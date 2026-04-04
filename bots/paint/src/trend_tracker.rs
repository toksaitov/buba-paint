use std::collections::HashMap;

use crate::portfolio::StrategyFamily;
use crate::types::SignalDirection;

/// A single recorded trade outcome.
#[derive(Debug)]
struct Outcome {
    direction: SignalDirection,
    won: bool,
    #[allow(dead_code)]
    timestamp: u64,
}

/// Tracks recent trade outcomes per direction and computes a trend bias.
///
/// The bias is `up_win_rate - down_win_rate`.  A positive bias means UP
/// trades have been winning more often; a negative bias means DOWN trades
/// have been winning more often.
///
/// When the filter is enabled, signals that go against the trend by more
/// than `filter_threshold` are suppressed.
#[derive(Debug)]
pub struct TrendTracker {
    recent_outcomes: Vec<Outcome>,
    window_size: usize,
    filter_enabled: bool,
    filter_threshold: f64,
}

#[derive(Debug)]
pub struct ScopedTrendTracker {
    global: TrendTracker,
    per_strategy: HashMap<StrategyFamily, TrendTracker>,
    scoped: bool,
    window_size: usize,
    filter_enabled: bool,
    filter_threshold: f64,
}

impl TrendTracker {
    /// Create a new trend tracker.
    ///
    /// * `window_size`       – maximum number of recent outcomes to keep.
    /// * `filter_enabled`    – whether `should_suppress` actually suppresses.
    /// * `filter_threshold`  – bias magnitude above which suppression fires.
    pub fn new(window_size: usize, filter_enabled: bool, filter_threshold: f64) -> Self {
        Self {
            recent_outcomes: Vec::new(),
            window_size,
            filter_enabled,
            filter_threshold,
        }
    }

    /// Record a trade outcome.  Older outcomes beyond `window_size` are
    /// dropped from the front of the buffer.
    pub fn record_outcome(&mut self, direction: SignalDirection, won: bool, now: u64) {
        self.recent_outcomes.push(Outcome {
            direction,
            won,
            timestamp: now,
        });
        if self.recent_outcomes.len() > self.window_size {
            self.recent_outcomes.remove(0);
        }
    }

    /// Compute the trend bias: `up_win_rate - down_win_rate`.
    ///
    /// Returns `0.0` if fewer than 3 outcomes have been recorded.  If a
    /// direction has no outcomes its win rate defaults to `0.5`.
    pub fn get_trend_bias(&self) -> f64 {
        if self.recent_outcomes.len() < 3 {
            return 0.0;
        }

        let mut up_wins: u32 = 0;
        let mut up_total: u32 = 0;
        let mut down_wins: u32 = 0;
        let mut down_total: u32 = 0;

        for o in &self.recent_outcomes {
            match o.direction {
                SignalDirection::Up => {
                    up_total += 1;
                    if o.won {
                        up_wins += 1;
                    }
                }
                SignalDirection::Down => {
                    down_total += 1;
                    if o.won {
                        down_wins += 1;
                    }
                }
            }
        }

        let up_rate = if up_total > 0 {
            f64::from(up_wins) / f64::from(up_total)
        } else {
            0.5
        };
        let down_rate = if down_total > 0 {
            f64::from(down_wins) / f64::from(down_total)
        } else {
            0.5
        };

        up_rate - down_rate
    }

    /// Returns `true` if the given direction should be suppressed based on
    /// the current trend bias.
    ///
    /// * An UP signal is suppressed when bias < -threshold (DOWN is winning).
    /// * A DOWN signal is suppressed when bias > +threshold (UP is winning).
    /// * Always returns `false` when the filter is disabled.
    pub fn should_suppress(&self, direction: SignalDirection) -> bool {
        if !self.filter_enabled {
            return false;
        }
        let bias = self.get_trend_bias();
        match direction {
            SignalDirection::Up => bias < -self.filter_threshold,
            SignalDirection::Down => bias > self.filter_threshold,
        }
    }
}

impl ScopedTrendTracker {
    /// Create one trend-tracker wrapper that can run either globally or per strategy.
    pub fn new(
        window_size: usize,
        filter_enabled: bool,
        filter_threshold: f64,
        scoped: bool,
    ) -> Self {
        Self {
            global: TrendTracker::new(window_size, filter_enabled, filter_threshold),
            per_strategy: HashMap::new(),
            scoped,
            window_size,
            filter_enabled,
            filter_threshold,
        }
    }

    /// Record one trade outcome either globally or for the provided family sleeve.
    pub fn record_outcome(
        &mut self,
        family: StrategyFamily,
        direction: SignalDirection,
        won: bool,
        now: u64,
    ) {
        if self.scoped {
            self.per_strategy
                .entry(family)
                .or_insert_with(|| {
                    TrendTracker::new(self.window_size, self.filter_enabled, self.filter_threshold)
                })
                .record_outcome(direction, won, now);
        } else {
            self.global.record_outcome(direction, won, now);
        }
    }

    /// Return whether a signal should be suppressed under the active scope mode.
    pub fn should_suppress(&mut self, family: StrategyFamily, direction: SignalDirection) -> bool {
        if self.scoped {
            return self
                .per_strategy
                .entry(family)
                .or_insert_with(|| {
                    TrendTracker::new(self.window_size, self.filter_enabled, self.filter_threshold)
                })
                .should_suppress(direction);
        }

        self.global.should_suppress(direction)
    }
}

#[cfg(test)]
#[path = "tests/trend_tracker_tests.rs"]
mod tests;
