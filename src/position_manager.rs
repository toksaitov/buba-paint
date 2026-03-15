// Position manager — manages trade lifecycle (open / close / settle).
//
// Ported from the TypeScript `PositionManager` class.  Instead of extending
// `EventEmitter`, methods return values that the caller feeds to the circuit
// breaker and trend tracker.

use crate::bankroll::BankrollManager;
use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::Database;
use crate::types::{
    MarketWindow, Signal, SignalDirection, SimulatedTrade, TradeResult, TradeStatus,
};

/// Manages the lifecycle of simulated (paper) trades.
///
/// Tracks how many positions are currently open and enforces the
/// `max_open_positions` limit from [`Config`].  Settlement results are
/// returned to the caller instead of being emitted via events.
pub struct PositionManager {
    open_count: u64,
}

impl Default for PositionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionManager {
    pub fn new() -> Self {
        Self { open_count: 0 }
    }

    /// Try to open a single position for the given signal.
    ///
    /// Returns `Some(trade)` on success, `None` if the position was blocked
    /// by any guard (max positions, duplicate strategy, insufficient capital).
    ///
    /// When `is_batch` is `true` (spread-capture), only exact duplicates
    /// (same strategy + same direction) are blocked.  When `false`, *any*
    /// existing position from the same strategy in the same market is blocked.
    #[allow(clippy::too_many_arguments)]
    pub fn try_open(
        &mut self,
        signal: &Signal,
        window: &MarketWindow,
        is_batch: bool,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Option<SimulatedTrade> {
        // Guard: max open positions.
        if self.open_count >= config.max_open_positions {
            return None;
        }

        // Guard: bankroll allows trading.
        if !bankroll.can_trade(config, clock) {
            return None;
        }

        // Guard: duplicate position in the same market.
        let existing = db.get_open_trades_for_market(&window.market_id).ok()?;
        if is_batch {
            // Block exact duplicates (same strategy + same direction).
            let duplicate = existing
                .iter()
                .any(|t| t.strategy == signal.strategy && t.side == signal.direction);
            if duplicate {
                return None;
            }
        } else {
            // Block ANY position from the same strategy.
            let same_strategy = existing.iter().any(|t| t.strategy == signal.strategy);
            if same_strategy {
                return None;
            }
        }

        // Determine entry price and token ID based on direction.
        let entry_price = match signal.direction {
            SignalDirection::Up => signal.up_ask,
            SignalDirection::Down => signal.down_ask,
        };
        let token_id = match signal.direction {
            SignalDirection::Up => window.up_token_id.clone(),
            SignalDirection::Down => window.down_token_id.clone(),
        };

        // Reserve capital via the bankroll manager.
        let size = bankroll.reserve_capital(
            entry_price,
            signal.confidence,
            &signal.strategy,
            config,
            clock,
        );
        if size <= 0.0 {
            return None;
        }

        let mut trade = SimulatedTrade {
            id: None,
            timestamp: signal.timestamp,
            market_id: window.market_id.clone(),
            strategy: signal.strategy.clone(),
            side: signal.direction,
            token_id,
            entry_price,
            size,
            status: TradeStatus::Open,
        };

        let id = db.open_trade(&trade).ok()?;
        trade.id = Some(id);
        self.open_count += 1;

        Some(trade)
    }

    /// Try to open a spread (both UP and DOWN legs) for the given signals.
    ///
    /// Returns a `Vec` of exactly two trades on success, or an empty `Vec` if
    /// any guard blocks the spread.
    #[allow(clippy::too_many_arguments)]
    pub fn try_open_spread(
        &mut self,
        signals: &[Signal],
        window: &MarketWindow,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Vec<SimulatedTrade> {
        // Need room for 2 more positions.
        if self.open_count + 2 > config.max_open_positions {
            return Vec::new();
        }

        if !bankroll.can_trade(config, clock) {
            return Vec::new();
        }

        // Find the UP and DOWN signals.
        let up_signal = signals.iter().find(|s| s.direction == SignalDirection::Up);
        let down_signal = signals
            .iter()
            .find(|s| s.direction == SignalDirection::Down);

        let (Some(up_signal), Some(down_signal)) = (up_signal, down_signal) else {
            return Vec::new();
        };

        // Check for existing duplicates.
        let Ok(existing) = db.get_open_trades_for_market(&window.market_id) else {
            return Vec::new();
        };

        for signal in [up_signal, down_signal] {
            let dup = existing
                .iter()
                .any(|t| t.strategy == signal.strategy && t.side == signal.direction);
            if dup {
                return Vec::new();
            }
        }

        // Reserve capital for both legs.
        let (up_tokens, down_tokens) = bankroll.reserve_spread_capital(
            up_signal.up_ask,
            down_signal.down_ask,
            up_signal.confidence,
            config,
            clock,
        );
        if up_tokens <= 0.0 || down_tokens <= 0.0 {
            return Vec::new();
        }

        let legs: [(&Signal, f64); 2] = [(up_signal, up_tokens), (down_signal, down_tokens)];
        let mut trades = Vec::with_capacity(2);

        for (signal, size) in legs {
            let entry_price = match signal.direction {
                SignalDirection::Up => signal.up_ask,
                SignalDirection::Down => signal.down_ask,
            };
            let token_id = match signal.direction {
                SignalDirection::Up => window.up_token_id.clone(),
                SignalDirection::Down => window.down_token_id.clone(),
            };

            let mut trade = SimulatedTrade {
                id: None,
                timestamp: signal.timestamp,
                market_id: window.market_id.clone(),
                strategy: signal.strategy.clone(),
                side: signal.direction,
                token_id,
                entry_price,
                size,
                status: TradeStatus::Open,
            };

            match db.open_trade(&trade) {
                Ok(id) => {
                    trade.id = Some(id);
                    self.open_count += 1;
                    trades.push(trade);
                }
                Err(_) => return Vec::new(),
            }
        }

        trades
    }

    /// Settle all open trades in a resolved market window.
    ///
    /// Returns a `Vec` of `(trade, result)` pairs.  The caller is responsible
    /// for feeding these to the circuit breaker and trend tracker.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_window(
        &mut self,
        window: &MarketWindow,
        open_price: f64,
        close_price: f64,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Vec<(SimulatedTrade, TradeResult)> {
        let outcome = if close_price >= open_price {
            SignalDirection::Up
        } else {
            SignalDirection::Down
        };

        let Ok(trades) = db.get_open_trades_for_market(&window.market_id) else {
            let _ = db.resolve_market(&window.market_id, "resolved");
            return Vec::new();
        };

        if trades.is_empty() {
            let _ = db.resolve_market(&window.market_id, "resolved");
            return Vec::new();
        }

        let mut results = Vec::with_capacity(trades.len());

        for trade in trades {
            let Some(trade_id) = trade.id else {
                continue;
            };
            let won = trade.side == outcome;
            let settlement_price = if won { 1.0 } else { 0.0 };
            let gross = (settlement_price - trade.entry_price) * trade.size;
            let entry_cost = trade.entry_price * trade.size;

            let result = TradeResult {
                trade_id,
                exit_price: settlement_price,
                settlement_price,
                pnl_0pct: gross,
                pnl_1pct: gross - entry_cost * 0.01,
                pnl_2pct: gross - entry_cost * 0.02,
                pnl_3pct: gross - entry_cost * 0.03,
            };

            let _ = db.close_trade(trade_id, &result);

            if self.open_count > 0 {
                self.open_count -= 1;
            }

            bankroll.apply_trade_result(
                trade_id,
                trade.entry_price,
                trade.size,
                settlement_price,
                &trade.strategy,
                config,
                db,
                clock,
            );

            results.push((trade, result));
        }

        let _ = db.resolve_market(&window.market_id, "resolved");

        results
    }

    /// Current number of open positions.
    pub fn open_count(&self) -> u64 {
        self.open_count
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/position_manager_tests.rs"]
mod tests;
