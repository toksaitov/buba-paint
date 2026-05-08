use std::collections::HashMap;

use tracing::warn;

use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::{Database, UnresolvedTradeExposure};
use crate::portfolio::StrategyFamily;

/// Structured reasons why a spread bundle is not queueable before submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SpreadAffordabilityFailureReason {
    SpreadBudgetTooSmall,
    BelowMarketMinSizeOnSubmit,
}

impl SpreadAffordabilityFailureReason {
    #[must_use]
    /// Return the stable persisted label for one pre-signal spread-affordability failure.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpreadBudgetTooSmall => "spread_budget_too_small",
            Self::BelowMarketMinSizeOnSubmit => "below_market_min_size_on_submit",
        }
    }
}

/// Result of a pure spread-affordability check against the current bankroll state.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct SpreadAffordabilityCheck {
    pub queueable: bool,
    pub available_spread_budget: f64,
    pub required_pair_units: f64,
    pub required_pair_notional: f64,
    pub failure_reason: Option<SpreadAffordabilityFailureReason>,
}

/// Rolling per-strategy win/loss counts used for Kelly sizing.
#[derive(Debug, Clone)]
struct StrategyRecord {
    wins: u64,
    losses: u64,
}

/// One recent trade result kept for rolling strategy attribution.
#[derive(Debug, Clone)]
struct TradeResultRecord {
    strategy: String,
    won: bool,
}

/// Snapshot of key bankroll statistics.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct BankrollStats {
    pub starting_balance: f64,
    pub current_balance: f64,
    pub high_water_mark: f64,
    pub max_drawdown_pct: f64,
    pub total_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub total_fees: f64,
}

#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct BankrollManager {
    starting_balance: f64,
    current_balance: f64,
    high_water_mark: f64,
    peak_drawdown_pct: f64,
    total_wins: u64,
    total_losses: u64,
    total_trades: u64,
    active_reserved_capital: f64,
    pending_settlement_reserved_capital: f64,
    reserved_capital: f64,
    active_reserved_capital_by_family: HashMap<StrategyFamily, f64>,
    pending_settlement_reserved_capital_by_family: HashMap<StrategyFamily, f64>,
    reserved_capital_by_family: HashMap<StrategyFamily, f64>,
    peak_dd_pause_until: u64,
    dd_pause_armed: bool,
    total_fees: f64,
    strategy_stats: HashMap<String, StrategyRecord>,
    recent_results: Vec<TradeResultRecord>,
    kelly_rolling_window: usize,
    /// Timestamp of the last rate-limited "trading blocked" log message.
    pub last_blocked_log_ms: u64,
}

impl BankrollManager {
    /// Construct a new `BankrollManager`.
    ///
    /// If the database already contains a balance-log entry the manager
    /// recovers from that value; otherwise it writes the initial `"init"` event.
    pub fn new(starting_balance: f64, config: &Config, db: &Database, clock: &dyn Clock) -> Self {
        let recovered = db.get_latest_balance().unwrap_or(None);
        let current = if let Some(bal) = recovered {
            bal
        } else {
            let _ = db.log_balance_event(clock.now(), "init", None, 0.0, starting_balance);
            starting_balance
        };

        let mut manager = Self::new_in_memory(starting_balance, current, config);
        manager.hydrate_unresolved_reserves(db, config, clock.now());
        manager
    }

    /// Construct a `BankrollManager` without reading or writing storage.
    #[must_use]
    pub fn new_in_memory(starting_balance: f64, current_balance: f64, config: &Config) -> Self {
        Self {
            starting_balance,
            current_balance,
            high_water_mark: starting_balance.max(current_balance),
            peak_drawdown_pct: 0.0,
            total_wins: 0,
            total_losses: 0,
            total_trades: 0,
            active_reserved_capital: 0.0,
            pending_settlement_reserved_capital: 0.0,
            reserved_capital: 0.0,
            active_reserved_capital_by_family: HashMap::new(),
            pending_settlement_reserved_capital_by_family: HashMap::new(),
            reserved_capital_by_family: HashMap::new(),
            peak_dd_pause_until: 0,
            dd_pause_armed: true,
            total_fees: 0.0,
            strategy_stats: HashMap::new(),
            recent_results: Vec::new(),
            kelly_rolling_window: config.kelly_rolling_window as usize,
            last_blocked_log_ms: 0,
        }
    }

    /// Hydrate reserve buckets from unresolved trade exposures loaded outside the hot path.
    pub fn hydrate_unresolved_exposure_rows(
        &mut self,
        exposures: &[UnresolvedTradeExposure],
        config: &Config,
        now_ms: u64,
    ) {
        for exposure in exposures {
            self.hydrate_trade_reserve(exposure, now_ms);
        }
        if !exposures.is_empty() && self.effective_reserved_capital(config) > 0.0 {
            tracing::info!(
                unresolved_trades = exposures.len(),
                active_reserved = self.active_reserved_capital,
                pending_reserved = self.pending_settlement_reserved_capital,
                effective_reserved = self.effective_reserved_capital(config),
                "rehydrated unresolved trade reserve state"
            );
        }
    }

    /// Reserve capital for a single-side (latency-arb) trade.
    ///
    /// Returns the token count (an integer kept as `f64`), or `0.0` when the
    /// trade should not be placed.
    pub fn reserve_capital(
        &mut self,
        entry_price: f64,
        confidence: f64,
        strategy: &str,
        config: &Config,
        clock: &dyn Clock,
    ) -> f64 {
        self.reserve_capital_with_reserve_price(
            entry_price,
            entry_price,
            confidence,
            strategy,
            family_for_strategy(strategy),
            config,
            clock,
        )
    }

    /// Reserve capital for a single-side trade using one price for sizing and
    /// another for worst-case capital reservation.
    ///
    /// The returned token count is sized from `entry_price` but must still be
    /// affordable at `reserve_price`.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_capital_with_reserve_price(
        &mut self,
        entry_price: f64,
        reserve_price: f64,
        confidence: f64,
        strategy: &str,
        family: StrategyFamily,
        config: &Config,
        clock: &dyn Clock,
    ) -> f64 {
        if !self.can_trade(config, clock) {
            return 0.0;
        }
        if entry_price <= 0.0 || entry_price >= 1.0 || reserve_price <= 0.0 || reserve_price >= 1.0
        {
            return 0.0;
        }

        let available = self
            .available_single_budget(family, config)
            .min(self.current_balance - self.effective_reserved_capital(config));
        if available <= 0.0 {
            return 0.0;
        }

        let fraction =
            self.get_position_fraction(entry_price, confidence, strategy, family, config);
        if fraction <= 0.0 {
            return 0.0;
        }

        let kelly_notional = self.current_balance * fraction;
        let max_position_usd = self.current_balance * config.max_position_usd_fraction;
        let notional = kelly_notional
            .min(available)
            .min(max_position_usd)
            .min(config.max_position_usd);

        let mut token_count = (notional / reserve_price).floor();

        if token_count > 0.0 && token_count * entry_price < config.min_bet_usd {
            let min_tokens = (config.min_bet_usd / entry_price).ceil();
            if min_tokens * reserve_price <= available
                && min_tokens * reserve_price <= max_position_usd
                && min_tokens * reserve_price <= config.max_position_usd
            {
                token_count = min_tokens;
            }
        }

        if token_count <= 0.0 {
            return 0.0;
        }

        let cost = token_count * reserve_price;
        self.add_reserved(family, cost);
        token_count
    }

    /// Reserve capital for a spread-capture (buy both sides) trade.
    ///
    /// Returns `(up_tokens, down_tokens)` — both equal for a balanced pair.
    pub fn reserve_spread_capital(
        &mut self,
        up_ask: f64,
        down_ask: f64,
        confidence: f64,
        config: &Config,
        clock: &dyn Clock,
    ) -> (f64, f64) {
        self.reserve_spread_capital_for_family(
            up_ask,
            down_ask,
            confidence,
            StrategyFamily::SpreadCapture,
            config,
            clock,
        )
    }

    /// Reserve capital for a spread bundle using one explicit family sleeve.
    pub fn reserve_spread_capital_for_family(
        &mut self,
        up_ask: f64,
        down_ask: f64,
        confidence: f64,
        family: StrategyFamily,
        config: &Config,
        clock: &dyn Clock,
    ) -> (f64, f64) {
        let zero = (0.0, 0.0);

        if !self.can_trade(config, clock) {
            return zero;
        }
        if up_ask <= 0.0 || down_ask <= 0.0 || up_ask >= 1.0 || down_ask >= 1.0 {
            return zero;
        }

        let available = self
            .available_spread_budget_for_family(family, config)
            .min(self.current_balance - self.effective_reserved_capital(config));
        if available <= 0.0 {
            return zero;
        }

        let total_ask_per_unit = up_ask + down_ask;
        let notional = available;

        let pair_units = (notional / total_ask_per_unit).floor();
        if pair_units <= 0.0 {
            return zero;
        }

        let total_cost = pair_units * total_ask_per_unit;
        self.add_reserved(family, total_cost);

        let _ = confidence;

        (pair_units, pair_units)
    }

    /// Assess whether a spread bundle can be legally queued right now without
    /// mutating reserved capital or other bankroll state.
    #[must_use]
    pub fn assess_spread_affordability(
        &self,
        up_ask: f64,
        down_ask: f64,
        order_min_size: f64,
        config: &Config,
    ) -> SpreadAffordabilityCheck {
        self.assess_spread_affordability_for_family(
            up_ask,
            down_ask,
            order_min_size,
            StrategyFamily::SpreadCapture,
            config,
        )
    }

    /// Assess spread queueability against one explicit family sleeve.
    #[must_use]
    pub fn assess_spread_affordability_for_family(
        &self,
        up_ask: f64,
        down_ask: f64,
        order_min_size: f64,
        family: StrategyFamily,
        config: &Config,
    ) -> SpreadAffordabilityCheck {
        let available_spread_budget = self.available_spread_budget_for_family(family, config);
        let total_ask_per_unit = up_ask + down_ask;

        if up_ask <= 0.0
            || down_ask <= 0.0
            || up_ask >= 1.0
            || down_ask >= 1.0
            || total_ask_per_unit <= 0.0
        {
            return SpreadAffordabilityCheck {
                queueable: false,
                available_spread_budget,
                required_pair_units: 0.0,
                required_pair_notional: 0.0,
                failure_reason: Some(SpreadAffordabilityFailureReason::SpreadBudgetTooSmall),
            };
        }

        let min_pair_units_for_bet = (config.min_bet_usd / up_ask)
            .ceil()
            .max((config.min_bet_usd / down_ask).ceil());
        let min_pair_units_for_market = if order_min_size > 0.0 {
            order_min_size.ceil()
        } else {
            0.0
        };
        let required_pair_units = min_pair_units_for_bet
            .max(min_pair_units_for_market)
            .max(1.0);
        let required_pair_notional = required_pair_units * total_ask_per_unit;

        if available_spread_budget + 1e-9 >= required_pair_notional {
            return SpreadAffordabilityCheck {
                queueable: true,
                available_spread_budget,
                required_pair_units,
                required_pair_notional,
                failure_reason: None,
            };
        }

        let failure_reason = if min_pair_units_for_market > min_pair_units_for_bet {
            SpreadAffordabilityFailureReason::BelowMarketMinSizeOnSubmit
        } else {
            SpreadAffordabilityFailureReason::SpreadBudgetTooSmall
        };

        SpreadAffordabilityCheck {
            queueable: false,
            available_spread_budget,
            required_pair_units,
            required_pair_notional,
            failure_reason: Some(failure_reason),
        }
    }

    /// Record the result of a closed trade, updating balance, win/loss tallies,
    /// strategy stats, rolling window, high-water mark, and drawdown.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_trade_result(
        &mut self,
        trade_id: i64,
        entry_price: f64,
        size: f64,
        settlement_price: f64,
        fee_amount: f64,
        strategy: &str,
        config: &Config,
        db: &Database,
        clock: &dyn Clock,
    ) {
        let cost = entry_price * size;
        let payout = settlement_price * size;
        let pnl = payout - cost - fee_amount;

        self.release_reserved_for_strategy(cost, strategy);
        self.current_balance += pnl;
        self.total_fees += fee_amount;
        self.total_trades += 1;

        let won = pnl > 0.0;
        if won {
            self.total_wins += 1;
        } else {
            self.total_losses += 1;
        }

        let stats = self
            .strategy_stats
            .entry(strategy.to_string())
            .or_insert(StrategyRecord { wins: 0, losses: 0 });
        if won {
            stats.wins += 1;
        } else {
            stats.losses += 1;
        }

        self.recent_results.push(TradeResultRecord {
            strategy: strategy.to_string(),
            won,
        });
        if self.recent_results.len() > self.kelly_rolling_window {
            self.recent_results.remove(0);
        }

        if self.current_balance > self.high_water_mark {
            self.high_water_mark = self.current_balance;
        }

        let drawdown = self.get_drawdown_pct();
        if drawdown > self.peak_drawdown_pct {
            self.peak_drawdown_pct = drawdown;
        }

        let _ = db.log_balance_event(
            clock.now(),
            "trade_close",
            Some(trade_id),
            pnl,
            self.current_balance,
        );

        let _ = config;
    }

    /// Record a closed trade result in memory without touching storage.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_trade_result_in_memory(
        &mut self,
        entry_price: f64,
        size: f64,
        settlement_price: f64,
        fee_amount: f64,
        strategy: &str,
    ) -> f64 {
        let cost = entry_price * size;
        let payout = settlement_price * size;
        let pnl = payout - cost - fee_amount;

        self.release_reserved_for_strategy(cost, strategy);
        self.current_balance += pnl;
        self.total_fees += fee_amount;
        self.total_trades += 1;

        let won = pnl > 0.0;
        if won {
            self.total_wins += 1;
        } else {
            self.total_losses += 1;
        }

        let stats = self
            .strategy_stats
            .entry(strategy.to_string())
            .or_insert(StrategyRecord { wins: 0, losses: 0 });
        if won {
            stats.wins += 1;
        } else {
            stats.losses += 1;
        }

        self.recent_results.push(TradeResultRecord {
            strategy: strategy.to_string(),
            won,
        });
        if self.recent_results.len() > self.kelly_rolling_window {
            self.recent_results.remove(0);
        }

        if self.current_balance > self.high_water_mark {
            self.high_water_mark = self.current_balance;
        }

        let drawdown = self.get_drawdown_pct();
        if drawdown > self.peak_drawdown_pct {
            self.peak_drawdown_pct = drawdown;
        }

        pnl
    }

    /// Whether trading is allowed right now (balance, drawdown, peak-DD pause).
    pub fn can_trade(&mut self, config: &Config, clock: &dyn Clock) -> bool {
        let now = clock.now();

        if self.current_balance < config.min_balance_threshold {
            self.log_blocked("below minimum balance", now);
            return false;
        }
        if self.get_drawdown_pct() >= config.max_drawdown_pct {
            self.log_blocked("max drawdown exceeded", now);
            return false;
        }

        let peak_dd = self.get_drawdown_pct();

        let recovery_threshold = config.peak_dd_pause_pct - config.dd_pause_recovery_pct;
        if peak_dd < recovery_threshold {
            self.dd_pause_armed = true;
            self.peak_dd_pause_until = 0;
        }

        if peak_dd >= config.peak_dd_pause_pct && self.dd_pause_armed {
            if self.peak_dd_pause_until == 0 {
                self.peak_dd_pause_until = now + config.peak_dd_pause_ms;
            }
            if now < self.peak_dd_pause_until {
                self.log_blocked("peak DD pause active", now);
                return false;
            }

            self.peak_dd_pause_until = 0;
            self.dd_pause_armed = false;
        } else if peak_dd < config.peak_dd_pause_pct {
            self.peak_dd_pause_until = 0;
        }

        true
    }

    /// Apply a settlement correction when the authoritative Polymarket outcome
    /// disagrees with the provisional Binance-based settlement.
    ///
    /// `old_pnl` is what was applied during provisional settlement.
    /// `new_pnl` is the corrected profit/loss from the authoritative outcome.
    /// The balance is adjusted by `new_pnl - old_pnl`.
    pub fn apply_settlement_correction(
        &mut self,
        trade_id: i64,
        old_pnl: f64,
        new_pnl: f64,
        db: &Database,
        clock: &dyn Clock,
    ) {
        let delta = new_pnl - old_pnl;
        self.current_balance += delta;
        self.total_fees = (self.total_fees + delta).max(0.0);

        let was_win = old_pnl > 0.0;
        let is_win = new_pnl > 0.0;
        if was_win && !is_win {
            if self.total_wins > 0 {
                self.total_wins -= 1;
            }
            self.total_losses += 1;
        } else if !was_win && is_win {
            if self.total_losses > 0 {
                self.total_losses -= 1;
            }
            self.total_wins += 1;
        }

        if self.current_balance > self.high_water_mark {
            self.high_water_mark = self.current_balance;
        }

        let dd = self.get_drawdown_pct();
        if dd > self.peak_drawdown_pct {
            self.peak_drawdown_pct = dd;
        }

        let _ = db.log_balance_event(
            clock.now(),
            "settlement_correction",
            Some(trade_id),
            delta,
            self.current_balance,
        );
    }

    /// Release previously reserved capital (used when liquidity clamping
    /// reduces a trade size below what was originally reserved).
    pub fn release_reserved(&mut self, amount: f64) {
        self.release_from_global_buckets(amount);
    }

    /// Release previously reserved capital for one strategy family.
    pub fn release_reserved_for_strategy(&mut self, amount: f64, strategy: &str) {
        self.release_reserved_for_family(amount, family_for_strategy(strategy));
    }

    /// Release previously reserved capital for one explicit family sleeve.
    pub fn release_reserved_for_family(&mut self, amount: f64, family: StrategyFamily) {
        self.release_from_family_buckets(amount, family);
    }

    /// Move one unresolved trade's reserve from active-market risk to pending settlement.
    pub fn transition_trade_to_pending_settlement(&mut self, amount: f64, strategy: &str) {
        self.transition_family_reserve_to_pending(amount, family_for_strategy(strategy));
    }

    /// Current balance.
    pub fn get_balance(&self) -> f64 {
        self.current_balance
    }

    /// Overall win rate across all trades.
    pub fn get_win_rate(&self) -> f64 {
        if self.total_trades > 0 {
            self.total_wins as f64 / self.total_trades as f64
        } else {
            0.0
        }
    }

    /// Current drawdown as a fraction of the high-water mark.
    pub fn get_drawdown_pct(&self) -> f64 {
        if self.high_water_mark <= 0.0 {
            return 0.0;
        }
        (self.high_water_mark - self.current_balance) / self.high_water_mark
    }

    /// Win rate for a specific strategy, preferring the rolling window when
    /// it has at least 5 results for that strategy, otherwise falling back
    /// to lifetime stats.
    pub fn get_strategy_win_rate(&self, strategy: &str) -> f64 {
        let rolling_for_strategy: Vec<&TradeResultRecord> = self
            .recent_results
            .iter()
            .filter(|r| r.strategy == strategy)
            .collect();

        if rolling_for_strategy.len() >= 5 {
            let wins = rolling_for_strategy.iter().filter(|r| r.won).count();
            return wins as f64 / rolling_for_strategy.len() as f64;
        }

        let Some(stats) = self.strategy_stats.get(strategy) else {
            return 0.0;
        };
        let total = stats.wins + stats.losses;
        if total > 0 {
            stats.wins as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Full statistics snapshot.
    pub fn get_stats(&self) -> BankrollStats {
        BankrollStats {
            starting_balance: self.starting_balance,
            current_balance: self.current_balance,
            high_water_mark: self.high_water_mark,
            max_drawdown_pct: self.peak_drawdown_pct,
            total_trades: self.total_trades,
            wins: self.total_wins,
            losses: self.total_losses,
            win_rate: self.get_win_rate(),
            total_pnl: self.current_balance - self.starting_balance,
            total_fees: self.total_fees,
        }
    }

    /// Rate-limited log when trading is blocked. Logs at most once per 60 seconds.
    fn log_blocked(&mut self, reason: &str, now: u64) {
        if self.last_blocked_log_ms == 0 || now.saturating_sub(self.last_blocked_log_ms) >= 60_000 {
            tracing::warn!(reason, "bankroll: trading blocked");
            self.last_blocked_log_ms = now;
        }
    }

    /// Compute the position fraction to risk on a single trade.
    fn get_position_fraction(
        &self,
        entry_price: f64,
        confidence: f64,
        strategy: &str,
        family: StrategyFamily,
        config: &Config,
    ) -> f64 {
        let strat_total = self
            .strategy_stats
            .get(strategy)
            .map_or(0, |s| s.wins + s.losses);

        let fraction = if strat_total >= config.min_trades_for_kelly {
            let win_rate = self.get_strategy_win_rate(strategy);
            self.get_kelly_fraction(entry_price, win_rate, config)
        } else {
            position_fraction_target(family, config)
        };

        let confidence_multiplier = (confidence - 0.5).mul_add(2.5, 0.0).max(0.0);
        let adjusted = fraction * confidence_multiplier;
        adjusted.min(position_fraction_target(family, config))
    }

    /// Half-Kelly fraction: `f* = (b*p - q) / b`, scaled by `KELLY_FRACTION`.
    #[allow(clippy::unused_self)]
    fn get_kelly_fraction(&self, entry_price: f64, win_rate: f64, config: &Config) -> f64 {
        if win_rate < config.min_win_rate_for_kelly {
            return config.min_kelly_floor;
        }

        let b = (1.0 - entry_price) / entry_price;
        let p = win_rate;
        let q = 1.0 - p;
        let full_kelly = (b * p - q) / b;

        if full_kelly <= 0.0 {
            return config.min_kelly_floor;
        }

        full_kelly * config.kelly_fraction
    }

    /// Return the current maximum notional budget available for one spread
    /// bundle in one family sleeve.
    #[must_use]
    fn available_spread_budget_for_family(&self, family: StrategyFamily, config: &Config) -> f64 {
        let available = (self.current_balance - self.effective_reserved_capital(config)).max(0.0);
        let max_position_usd = self.current_balance * config.max_position_usd_fraction;
        let per_trade_cap = self.current_balance * position_fraction_target(family, config);
        let mut budget = available
            .min(max_position_usd)
            .min(config.max_position_usd)
            .min(per_trade_cap);

        if let Some(family_position_fraction) = explicit_family_sleeve_fraction(family, config) {
            let family_reserved = self.effective_reserved_for_family(family, config);
            let family_budget =
                (self.current_balance * family_position_fraction - family_reserved).max(0.0);
            budget = budget.min(family_budget);
        }

        budget.max(0.0)
    }

    /// Return the current maximum notional budget available for one single-side
    /// order in one family sleeve.
    #[must_use]
    fn available_single_budget(&self, family: StrategyFamily, config: &Config) -> f64 {
        self.available_spread_budget_for_family(family, config)
    }

    /// Add newly reserved notional to the global and family-scoped counters.
    fn add_reserved(&mut self, family: StrategyFamily, amount: f64) {
        self.active_reserved_capital += amount;
        *self
            .active_reserved_capital_by_family
            .entry(family)
            .or_insert(0.0) += amount;
        self.sync_raw_reserved_totals();
    }

    /// Recover unresolved-trade reserve state from the existing run database.
    fn hydrate_unresolved_reserves(&mut self, db: &Database, config: &Config, now_ms: u64) {
        let unresolved = match db.unresolved_trade_exposures() {
            Ok(exposures) => exposures,
            Err(error) => {
                warn!("failed to hydrate unresolved reserves from DB: {error}");
                return;
            }
        };
        let unresolved_count = unresolved.len();

        for exposure in unresolved {
            self.hydrate_trade_reserve(&exposure, now_ms);
        }

        if unresolved_count > 0 && self.effective_reserved_capital(config) > 0.0 {
            tracing::info!(
                unresolved_trades = unresolved_count,
                active_reserved = self.active_reserved_capital,
                pending_reserved = self.pending_settlement_reserved_capital,
                effective_reserved = self.effective_reserved_capital(config),
                "rehydrated unresolved trade reserve state"
            );
        }
    }

    /// Apply one unresolved trade exposure to the in-memory reserve buckets.
    fn hydrate_trade_reserve(&mut self, exposure: &UnresolvedTradeExposure, now_ms: u64) {
        let family = family_for_strategy(&exposure.strategy);
        let amount = exposure.entry_price * exposure.size;
        if exposure.market_end_time <= now_ms {
            self.pending_settlement_reserved_capital += amount;
            *self
                .pending_settlement_reserved_capital_by_family
                .entry(family)
                .or_insert(0.0) += amount;
        } else {
            self.active_reserved_capital += amount;
            *self
                .active_reserved_capital_by_family
                .entry(family)
                .or_insert(0.0) += amount;
        }
        self.sync_raw_reserved_totals();
    }

    /// Return the config-weighted global reserved amount used for live capital checks.
    fn effective_reserved_capital(&self, config: &Config) -> f64 {
        let pending_policy = config.pending_settlement_policy_unchecked();
        let split_total = self.active_reserved_capital + self.pending_settlement_reserved_capital;
        if (self.reserved_capital - split_total).abs() > 1e-9 {
            return self.reserved_capital.max(0.0);
        }
        self.active_reserved_capital
            + self.pending_settlement_reserved_capital * pending_policy.global_reserve_fraction
    }

    /// Return the config-weighted family reserve used for sleeve checks.
    fn effective_reserved_for_family(&self, family: StrategyFamily, config: &Config) -> f64 {
        let pending_policy = config.pending_settlement_policy_unchecked();
        let active = self
            .active_reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        let pending = self
            .pending_settlement_reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        let split_total = active + pending;
        let total = self
            .reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        if (total - split_total).abs() > 1e-9 {
            return total.max(0.0);
        }
        active + pending * pending_policy.family_reserve_fraction
    }

    /// Move one family's raw reserve from active-market risk into pending settlement.
    fn transition_family_reserve_to_pending(&mut self, amount: f64, family: StrategyFamily) {
        if self.raw_reserve_state_is_manually_overridden_for_family(family) {
            return;
        }
        let moved = self.consume_family_active_reserve(amount, family);
        if moved <= 0.0 {
            return;
        }
        self.pending_settlement_reserved_capital += moved;
        *self
            .pending_settlement_reserved_capital_by_family
            .entry(family)
            .or_insert(0.0) += moved;
        self.sync_raw_reserved_totals();
    }

    /// Release reserve from the active bucket first, then pending settlement.
    fn release_from_global_buckets(&mut self, amount: f64) {
        if self.raw_reserve_state_is_manually_overridden() {
            self.reserved_capital = (self.reserved_capital - amount).max(0.0);
            return;
        }
        let remaining = self.consume_across_families(amount, true);
        if remaining > 0.0 {
            let _ = self.consume_across_families(remaining, false);
        }
        self.sync_raw_reserved_totals();
    }

    /// Release reserve for one family from active first, then pending settlement.
    fn release_from_family_buckets(&mut self, amount: f64, family: StrategyFamily) {
        if self.raw_reserve_state_is_manually_overridden_for_family(family) {
            self.reserved_capital = (self.reserved_capital - amount).max(0.0);
            if let Some(entry) = self.reserved_capital_by_family.get_mut(&family) {
                *entry = (*entry - amount).max(0.0);
            }
            return;
        }
        let active_released = self.consume_family_active_reserve(amount, family);
        let remaining = (amount - active_released).max(0.0);
        if remaining > 0.0 {
            self.consume_family_pending_reserve(remaining, family);
        }
        self.sync_raw_reserved_totals();
    }

    /// Consume up to `amount` from one family's active-market reserve bucket.
    fn consume_family_active_reserve(&mut self, amount: f64, family: StrategyFamily) -> f64 {
        let current = self
            .active_reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        let released = amount.min(current);
        if released > 0.0 {
            self.active_reserved_capital = (self.active_reserved_capital - released).max(0.0);
            if let Some(entry) = self.active_reserved_capital_by_family.get_mut(&family) {
                *entry = (*entry - released).max(0.0);
            }
        }
        released
    }

    /// Consume up to `amount` from one family's pending-settlement reserve bucket.
    fn consume_family_pending_reserve(&mut self, amount: f64, family: StrategyFamily) -> f64 {
        let current = self
            .pending_settlement_reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        let released = amount.min(current);
        if released > 0.0 {
            self.pending_settlement_reserved_capital =
                (self.pending_settlement_reserved_capital - released).max(0.0);
            if let Some(entry) = self
                .pending_settlement_reserved_capital_by_family
                .get_mut(&family)
            {
                *entry = (*entry - released).max(0.0);
            }
        }
        released
    }

    /// Consume reserve across every family in a stable order for family-less releases.
    fn consume_across_families(&mut self, amount: f64, active_bucket: bool) -> f64 {
        let mut remaining = amount.max(0.0);
        for family in [
            StrategyFamily::LatencyArb,
            StrategyFamily::SpreadCapture,
            StrategyFamily::CalmPersistence,
        ] {
            if remaining <= 0.0 {
                break;
            }
            let released = if active_bucket {
                self.consume_family_active_reserve(remaining, family)
            } else {
                self.consume_family_pending_reserve(remaining, family)
            };
            remaining = (remaining - released).max(0.0);
        }
        remaining
    }

    /// Return whether legacy callers mutated the raw reserve total without the split buckets.
    fn raw_reserve_state_is_manually_overridden(&self) -> bool {
        let split_total = self.active_reserved_capital + self.pending_settlement_reserved_capital;
        (self.reserved_capital - split_total).abs() > 1e-9
    }

    /// Return whether one family's legacy raw reserve total no longer matches the split buckets.
    fn raw_reserve_state_is_manually_overridden_for_family(&self, family: StrategyFamily) -> bool {
        if self.raw_reserve_state_is_manually_overridden() {
            return true;
        }
        let split_total = self
            .active_reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0)
            + self
                .pending_settlement_reserved_capital_by_family
                .get(&family)
                .copied()
                .unwrap_or(0.0);
        let total = self
            .reserved_capital_by_family
            .get(&family)
            .copied()
            .unwrap_or(0.0);
        (total - split_total).abs() > 1e-9
    }

    /// Rebuild the legacy raw reserve totals from the active and pending buckets.
    fn sync_raw_reserved_totals(&mut self) {
        self.reserved_capital =
            self.active_reserved_capital + self.pending_settlement_reserved_capital;
        self.reserved_capital_by_family = [
            StrategyFamily::LatencyArb,
            StrategyFamily::SpreadCapture,
            StrategyFamily::CalmPersistence,
        ]
        .into_iter()
        .map(|family| {
            let active = self
                .active_reserved_capital_by_family
                .get(&family)
                .copied()
                .unwrap_or(0.0);
            let pending = self
                .pending_settlement_reserved_capital_by_family
                .get(&family)
                .copied()
                .unwrap_or(0.0);
            (family, active + pending)
        })
        .collect();
    }
}

/// Return the balance-fraction cap used by spread-capture sizing.
/// Return the spread position-fraction cap, falling back to the global control row.
fn spread_position_fraction(config: &Config) -> f64 {
    config
        .spread_capture_max_position_fraction
        .unwrap_or(config.max_position_fraction)
}

/// Return the explicit family sleeve fraction, if one has been configured.
fn explicit_family_sleeve_fraction(family: StrategyFamily, config: &Config) -> Option<f64> {
    match family {
        StrategyFamily::LatencyArb => config.latency_arb_max_position_fraction,
        StrategyFamily::SpreadCapture => config.spread_capture_max_position_fraction,
        StrategyFamily::CalmPersistence => config.calm_persistence_max_position_fraction,
    }
}

/// Return the per-trade position-fraction target for the provided strategy family.
fn position_fraction_target(family: StrategyFamily, config: &Config) -> f64 {
    match family {
        StrategyFamily::LatencyArb => config
            .latency_arb_max_position_fraction
            .unwrap_or(config.max_position_fraction),
        StrategyFamily::SpreadCapture => spread_position_fraction(config),
        StrategyFamily::CalmPersistence => config
            .calm_persistence_max_position_fraction
            .unwrap_or(config.max_position_fraction),
    }
}

/// Infer the portfolio family from one persisted strategy name.
fn family_for_strategy(strategy: &str) -> StrategyFamily {
    StrategyFamily::from_strategy_name(strategy).unwrap_or(StrategyFamily::LatencyArb)
}

#[cfg(test)]
#[path = "tests/bankroll_tests.rs"]
mod tests;
