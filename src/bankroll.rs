// Bankroll manager — full Kelly-criterion-based position sizing.
//
// Ported from the TypeScript `BankrollManager` class.  The Rust version does
// NOT store references to `Database` or `Clock`; they are passed as method
// parameters instead, which makes the struct trivially `Send` and avoids
// lifetime issues in the backtest engine.

use std::collections::HashMap;

use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::Database;

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct StrategyRecord {
    wins: u64,
    losses: u64,
}

#[derive(Debug, Clone)]
struct TradeResultRecord {
    strategy: String,
    won: bool,
}

/// Snapshot of key bankroll statistics.
#[derive(Debug, Clone)]
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
}

// ---------------------------------------------------------------------------
// BankrollManager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BankrollManager {
    starting_balance: f64,
    current_balance: f64,
    high_water_mark: f64,
    peak_drawdown_pct: f64,
    total_wins: u64,
    total_losses: u64,
    total_trades: u64,
    reserved_capital: f64,
    peak_dd_pause_until: u64,
    strategy_stats: HashMap<String, StrategyRecord>,
    recent_results: Vec<TradeResultRecord>,
    kelly_rolling_window: usize,
}

impl BankrollManager {
    /// Construct a new `BankrollManager`.
    ///
    /// If the database already contains a balance-log entry the manager
    /// recovers from that value; otherwise it writes the initial `"init"` event.
    pub fn new(starting_balance: f64, config: &Config, db: &Database, clock: &dyn Clock) -> Self {
        let recovered = db.get_latest_balance().unwrap_or(None);

        let (current, hwm) = if let Some(bal) = recovered {
            (bal, starting_balance.max(bal))
        } else {
            let _ = db.log_balance_event(clock.now(), "init", None, 0.0, starting_balance);
            (starting_balance, starting_balance)
        };

        Self {
            starting_balance,
            current_balance: current,
            high_water_mark: hwm,
            peak_drawdown_pct: 0.0,
            total_wins: 0,
            total_losses: 0,
            total_trades: 0,
            reserved_capital: 0.0,
            peak_dd_pause_until: 0,
            strategy_stats: HashMap::new(),
            recent_results: Vec::new(),
            kelly_rolling_window: config.kelly_rolling_window as usize,
        }
    }

    // -- Public API ----------------------------------------------------------

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
        if !self.can_trade(config, clock) {
            return 0.0;
        }
        if entry_price <= 0.0 || entry_price >= 1.0 {
            return 0.0;
        }

        let available = self.current_balance - self.reserved_capital;
        if available <= 0.0 {
            return 0.0;
        }

        let fraction = self.get_position_fraction(entry_price, confidence, strategy, config);
        if fraction <= 0.0 {
            return 0.0;
        }

        let kelly_notional = self.current_balance * fraction;
        let max_position_usd = self.current_balance * config.max_position_usd_fraction;
        let notional = kelly_notional.min(available).min(max_position_usd);

        let mut token_count = (notional / entry_price).floor();

        // Min-bet floor: if the cost is below the minimum, bump up.
        if token_count > 0.0 && token_count * entry_price < config.min_bet_usd {
            let min_tokens = (config.min_bet_usd / entry_price).floor();
            if min_tokens * entry_price <= available && min_tokens * entry_price <= max_position_usd
            {
                token_count = min_tokens;
            }
        }

        if token_count <= 0.0 {
            return 0.0;
        }

        let cost = token_count * entry_price;
        self.reserved_capital += cost;
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
        let zero = (0.0, 0.0);

        if !self.can_trade(config, clock) {
            return zero;
        }
        if up_ask <= 0.0 || down_ask <= 0.0 || up_ask >= 1.0 || down_ask >= 1.0 {
            return zero;
        }

        let available = self.current_balance - self.reserved_capital;
        if available <= 0.0 {
            return zero;
        }

        let total_ask_per_unit = up_ask + down_ask;
        let max_position_usd = self.current_balance * config.max_position_usd_fraction;
        let max_from_balance = self.current_balance * config.max_position_fraction;
        let notional = max_from_balance.min(available).min(max_position_usd);

        let pair_units = (notional / total_ask_per_unit).floor();
        if pair_units <= 0.0 {
            return zero;
        }

        let total_cost = pair_units * total_ask_per_unit;
        self.reserved_capital += total_cost;

        // `confidence` is accepted for API parity with TypeScript but is not
        // used by spread-capture sizing (intentional — matches TS behaviour).
        let _ = confidence;

        (pair_units, pair_units)
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
        strategy: &str,
        config: &Config,
        db: &Database,
        clock: &dyn Clock,
    ) {
        let cost = entry_price * size;
        let payout = settlement_price * size;
        let pnl = payout - cost;

        self.reserved_capital = (self.reserved_capital - cost).max(0.0);
        self.current_balance += pnl;
        self.total_trades += 1;

        let won = pnl > 0.0;
        if won {
            self.total_wins += 1;
        } else {
            self.total_losses += 1;
        }

        // Per-strategy stats.
        let stats = self
            .strategy_stats
            .entry(strategy.to_string())
            .or_insert(StrategyRecord { wins: 0, losses: 0 });
        if won {
            stats.wins += 1;
        } else {
            stats.losses += 1;
        }

        // Rolling window.
        self.recent_results.push(TradeResultRecord {
            strategy: strategy.to_string(),
            won,
        });
        if self.recent_results.len() > self.kelly_rolling_window {
            self.recent_results.remove(0);
        }

        // High-water mark.
        if self.current_balance > self.high_water_mark {
            self.high_water_mark = self.current_balance;
        }

        // Peak drawdown tracking.
        let drawdown = self.get_drawdown_pct();
        if drawdown > self.peak_drawdown_pct {
            self.peak_drawdown_pct = drawdown;
        }

        // Persist balance event.
        let _ = db.log_balance_event(
            clock.now(),
            "trade_close",
            Some(trade_id),
            pnl,
            self.current_balance,
        );

        // `config` accepted for API symmetry; not used beyond future-proofing.
        let _ = config;
    }

    /// Whether trading is allowed right now (balance, drawdown, peak-DD pause).
    pub fn can_trade(&mut self, config: &Config, clock: &dyn Clock) -> bool {
        if self.current_balance < config.min_balance_threshold {
            return false;
        }
        if self.get_drawdown_pct() >= config.max_drawdown_pct {
            return false;
        }

        // Peak drawdown pause.
        let peak_dd = self.get_drawdown_pct();
        let now = clock.now();

        if peak_dd >= config.peak_dd_pause_pct {
            if self.peak_dd_pause_until == 0 {
                self.peak_dd_pause_until = now + config.peak_dd_pause_ms;
            }
            if now < self.peak_dd_pause_until {
                return false;
            }
            // Timer expired — reset.
            self.peak_dd_pause_until = 0;
        } else {
            self.peak_dd_pause_until = 0;
        }

        true
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
        }
    }

    // -- Private helpers -----------------------------------------------------

    /// Compute the position fraction to risk on a single trade.
    fn get_position_fraction(
        &self,
        entry_price: f64,
        confidence: f64,
        strategy: &str,
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
            config.max_position_fraction
        };

        let confidence_multiplier = (confidence - 0.5).mul_add(2.5, 0.0).max(0.0);
        let adjusted = fraction * confidence_multiplier;
        adjusted.min(config.max_position_fraction)
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/bankroll_tests.rs"]
mod tests;
