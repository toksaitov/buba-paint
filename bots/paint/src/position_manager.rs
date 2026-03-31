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
    /// Create an empty position manager by default.
    fn default() -> Self {
        Self::new()
    }
}

impl PositionManager {
    /// Create a new position manager with no open trades tracked in memory.
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
        if !self.can_open_position(signal, window, is_batch, db, bankroll, config, clock)? {
            return None;
        }

        let (entry_price, token_id) = trade_entry(signal, window);
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

        size = clamp_to_liquidity(size, available_liquidity_tokens, entry_price, bankroll);
        if !size_meets_min_bet(size, entry_price, config.min_bet_usd) {
            log_min_bet_rejection(signal, size, entry_price, config.min_bet_usd);
            bankroll.release_reserved(size * entry_price);
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
            signal_id: None,
            requested_price: None,
            requested_size: None,
            filled_size: None,
            avg_fill_price: None,
            fill_status: None,
            fill_reason: None,
            fill_latency_ms: None,
            execution_group_id: None,
            execution_fidelity: None,
            execution_mode: None,
            order_id: None,
            fill_price: None,
        };

        let id = db.open_trade(&trade).ok()?;
        trade.id = Some(id);
        self.open_count += 1;

        Some(trade)
    }

    /// Return whether a new position may be opened for the provided signal.
    #[allow(clippy::too_many_arguments)]
    fn can_open_position(
        &self,
        signal: &Signal,
        window: &MarketWindow,
        is_batch: bool,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
    ) -> Option<bool> {
        if self.open_count >= config.max_open_positions {
            tracing::debug!(
                open = self.open_count,
                max = config.max_open_positions,
                "max open positions reached"
            );
            return Some(false);
        }
        if !bankroll.can_trade(config, clock) {
            tracing::debug!(
                strategy = %signal.strategy,
                "trade rejected: bankroll blocked"
            );
            return Some(false);
        }
        let existing = db.get_open_trades_for_market(&window.market_id).ok()?;
        if has_duplicate_position(&existing, signal, is_batch) {
            tracing::debug!(
                strategy = %signal.strategy,
                market = %window.market_id,
                "trade rejected: duplicate position"
            );
            return Some(false);
        }
        Some(true)
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
        if self.open_count + 2 > config.max_open_positions {
            return Vec::new();
        }

        if !bankroll.can_trade(config, clock) {
            return Vec::new();
        }

        let up_signal = signals.iter().find(|s| s.direction == SignalDirection::Up);
        let down_signal = signals
            .iter()
            .find(|s| s.direction == SignalDirection::Down);

        let (Some(up_signal), Some(down_signal)) = (up_signal, down_signal) else {
            return Vec::new();
        };

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
                signal_id: None,
                requested_price: None,
                requested_size: None,
                filled_size: None,
                avg_fill_price: None,
                fill_status: None,
                fill_reason: None,
                fill_latency_ms: None,
                execution_group_id: None,
                execution_fidelity: None,
                execution_mode: None,
                order_id: None,
                fill_price: None,
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

            let fee_params = crate::fees::resolve_fee_params(config, Some(window), window.end_time);
            let fee_amount = crate::fees::compute_taker_fee(
                trade.entry_price,
                trade.size,
                fee_params.fee_rate,
                fee_params.exponent,
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

/// Return whether the target position already exists in the open trade set.
fn has_duplicate_position(existing: &[SimulatedTrade], signal: &Signal, is_batch: bool) -> bool {
    if is_batch {
        existing
            .iter()
            .any(|trade| trade.strategy == signal.strategy && trade.side == signal.direction)
    } else {
        existing
            .iter()
            .any(|trade| trade.strategy == signal.strategy)
    }
}

/// Return the persisted trade entry price and token id for a signal direction.
fn trade_entry(signal: &Signal, window: &MarketWindow) -> (f64, String) {
    match signal.direction {
        SignalDirection::Up => (signal.up_ask, window.up_token_id.clone()),
        SignalDirection::Down => (signal.down_ask, window.down_token_id.clone()),
    }
}

/// Clamp a reserved size to the available order-book liquidity.
fn clamp_to_liquidity(
    reserved_size: f64,
    available_liquidity_tokens: f64,
    entry_price: f64,
    bankroll: &mut BankrollManager,
) -> f64 {
    if available_liquidity_tokens > 0.0 && reserved_size > available_liquidity_tokens {
        bankroll.release_reserved((reserved_size - available_liquidity_tokens) * entry_price);
        available_liquidity_tokens
    } else {
        reserved_size
    }
}

/// Return whether the clamped trade still clears the configured minimum bet.
fn size_meets_min_bet(size: f64, entry_price: f64, min_bet_usd: f64) -> bool {
    size * entry_price >= min_bet_usd
}

/// Log the rejection details for a trade that falls below the minimum bet.
fn log_min_bet_rejection(signal: &Signal, size: f64, entry_price: f64, min_bet_usd: f64) {
    tracing::debug!(
        strategy = %signal.strategy,
        clamped_usd = size * entry_price,
        min_bet = min_bet_usd,
        "trade rejected: below min bet after liquidity clamp"
    );
}

#[cfg(test)]
#[path = "tests/position_manager_tests.rs"]
mod tests;
