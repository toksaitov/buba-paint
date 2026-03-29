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
        available_liquidity_tokens: f64,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Option<SimulatedTrade> {
        // Guard: max open positions.
        if self.open_count >= config.max_open_positions {
            tracing::debug!(
                open = self.open_count,
                max = config.max_open_positions,
                "max open positions reached"
            );
            return None;
        }

        // Guard: bankroll allows trading.
        if !bankroll.can_trade(config, clock) {
            tracing::debug!(
                strategy = %signal.strategy,
                "trade rejected: bankroll blocked"
            );
            return None;
        }

        // Guard: duplicate position in the same market.
        let existing = db.get_open_trades_for_market(&window.market_id).ok()?;
        if is_batch {
            let duplicate = existing
                .iter()
                .any(|t| t.strategy == signal.strategy && t.side == signal.direction);
            if duplicate {
                tracing::debug!(
                    strategy = %signal.strategy,
                    market = %window.market_id,
                    "trade rejected: duplicate batch position"
                );
                return None;
            }
        } else {
            let same_strategy = existing.iter().any(|t| t.strategy == signal.strategy);
            if same_strategy {
                tracing::debug!(
                    strategy = %signal.strategy,
                    market = %window.market_id,
                    "trade rejected: duplicate strategy position"
                );
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
        let mut size = bankroll.reserve_capital(
            entry_price,
            signal.confidence,
            &signal.strategy,
            config,
            clock,
        );
        if size <= 0.0 {
            tracing::debug!(
                strategy = %signal.strategy,
                entry_price,
                "trade rejected: reserve_capital returned 0"
            );
            return None;
        }

        // Clamp to available order book liquidity.
        if available_liquidity_tokens > 0.0 && size > available_liquidity_tokens {
            let excess_cost = (size - available_liquidity_tokens) * entry_price;
            size = available_liquidity_tokens;
            bankroll.release_reserved(excess_cost);
        }

        // Check if clamped size is still above minimum bet.
        if size * entry_price < config.min_bet_usd {
            let cost = size * entry_price;
            tracing::debug!(
                strategy = %signal.strategy,
                clamped_usd = cost,
                min_bet = config.min_bet_usd,
                "trade rejected: below min bet after liquidity clamp"
            );
            bankroll.release_reserved(cost);
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

    /// Settle all open trades in a resolved market window using open/close
    /// price comparison (used by the backtester where the outcome is derived
    /// from Chainlink data).
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
        self.resolve_window_with_outcome(window, outcome, db, bankroll, config, clock)
    }

    /// Settle all open trades using a known authoritative outcome (used by the
    /// live bot after polling the Gamma API for the actual Polymarket resolution).
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_window_with_outcome(
        &mut self,
        window: &MarketWindow,
        outcome: SignalDirection,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Vec<(SimulatedTrade, TradeResult)> {
        let outcome_str = match outcome {
            SignalDirection::Up => "UP",
            SignalDirection::Down => "DOWN",
        };

        let Ok(trades) = db.get_open_trades_for_market(&window.market_id) else {
            let _ = db.resolve_market_with_outcome(&window.market_id, "resolved", outcome_str);
            return Vec::new();
        };

        if trades.is_empty() {
            let _ = db.resolve_market_with_outcome(&window.market_id, "resolved", outcome_str);
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

            let fee_amount = crate::fees::compute_taker_fee(
                trade.entry_price,
                trade.size,
                config.taker_fee_rate,
                config.taker_fee_exponent,
            );

            let result = TradeResult {
                trade_id,
                exit_price: settlement_price,
                settlement_price,
                pnl_0pct: gross,
                pnl_1pct: gross - entry_cost * 0.01,
                pnl_2pct: gross - entry_cost * 0.02,
                pnl_3pct: gross - entry_cost * 0.03,
                fee_amount,
                pnl_net: gross - fee_amount,
                settlement_status: "confirmed".to_string(),
                provisional_pnl: None,
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
                fee_amount,
                &trade.strategy,
                config,
                db,
                clock,
            );

            results.push((trade, result));
        }

        let _ = db.resolve_market_with_outcome(&window.market_id, "resolved", outcome_str);

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
