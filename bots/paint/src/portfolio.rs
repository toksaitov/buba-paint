use std::collections::HashMap;

use crate::config::Config;
use crate::executor::{OrderOutcomeDisposition, ProcessedOrderOutcome, SubmissionOutcome};
use crate::types::StrategyContext;

/// Stable strategy families used by the portfolio router and sleeve accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum StrategyFamily {
    LatencyArb,
    SpreadCapture,
    CalmPersistence,
}

impl StrategyFamily {
    #[must_use]
    /// Return the persisted label for one strategy family.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LatencyArb => "latency-arb",
            Self::SpreadCapture => "spread-capture",
            Self::CalmPersistence => "calm-persistence",
        }
    }

    #[must_use]
    /// Map one persisted strategy name to its portfolio family.
    pub fn from_strategy_name(strategy: &str) -> Option<Self> {
        match strategy {
            "latency-arb" => Some(Self::LatencyArb),
            "spread-capture" => Some(Self::SpreadCapture),
            "calm-persistence" => Some(Self::CalmPersistence),
            _ => None,
        }
    }
}

/// Coarse market regimes used to decide which strategy family is eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum PortfolioRegime {
    Dislocation,
    StructuralPair,
    Calm,
}

impl PortfolioRegime {
    #[must_use]
    /// Return the persisted label for one detected portfolio regime.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dislocation => "dislocation",
            Self::StructuralPair => "structural_pair",
            Self::Calm => "calm",
        }
    }
}

/// Stable reasons why the router blocked a candidate family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum RouterBlockReason {
    RegimeMismatch,
}

impl RouterBlockReason {
    #[must_use]
    /// Return the persisted label for one router block reason.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegimeMismatch => "regime_mismatch",
        }
    }
}

/// Router output for one evaluation snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct RouterSelection {
    pub regime: PortfolioRegime,
    pub selected_family: Option<StrategyFamily>,
}

#[must_use]
/// Detect the portfolio regime implied by the current feature snapshot.
pub fn detect_regime(ctx: &StrategyContext, config: &Config) -> PortfolioRegime {
    if looks_like_structural_pair(ctx, config) {
        return PortfolioRegime::StructuralPair;
    }

    if looks_like_calm_regime(ctx, config) {
        return PortfolioRegime::Calm;
    }

    PortfolioRegime::Dislocation
}

/// Return whether one strategy family is eligible inside the provided regime.
#[must_use]
pub fn family_matches_regime(family: StrategyFamily, regime: PortfolioRegime) -> bool {
    matches!(
        (family, regime),
        (StrategyFamily::LatencyArb, PortfolioRegime::Dislocation)
            | (
                StrategyFamily::SpreadCapture,
                PortfolioRegime::StructuralPair
            )
            | (StrategyFamily::CalmPersistence, PortfolioRegime::Calm)
    )
}

/// Select the single eligible family from the current candidate set.
#[must_use]
pub fn select_family_for_candidates(
    ctx: &StrategyContext,
    config: &Config,
    candidates: &[StrategyFamily],
) -> RouterSelection {
    let regime = detect_regime(ctx, config);

    let selected_family = candidates
        .iter()
        .copied()
        .find(|family| family_matches_regime(*family, regime));

    RouterSelection {
        regime,
        selected_family,
    }
}

/// Aggregate regime and routing attribution collected during one run.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct PortfolioAttributionStats {
    regime_counts: HashMap<PortfolioRegime, u64>,
    candidate_counts: HashMap<StrategyFamily, u64>,
    queued_counts: HashMap<PortfolioRegime, u64>,
    filled_counts: HashMap<PortfolioRegime, u64>,
    missed_counts: HashMap<PortfolioRegime, u64>,
    signal_regimes: HashMap<i64, PortfolioRegime>,
    signal_families: HashMap<i64, StrategyFamily>,
    pub router_blocked_count: u64,
    pub capital_blocked_count: u64,
    pub latency_spread_overlap_count: u64,
    pub latency_calm_overlap_count: u64,
    pub spread_calm_overlap_count: u64,
}

impl PortfolioAttributionStats {
    /// Record the regime observed on one evaluation snapshot.
    pub fn record_observed_regime(&mut self, regime: PortfolioRegime) {
        *self.regime_counts.entry(regime).or_insert(0) += 1;
    }

    /// Record the set of candidate families seen on one evaluation snapshot.
    pub fn record_candidates(&mut self, candidates: &[StrategyFamily]) {
        for family in candidates {
            *self.candidate_counts.entry(*family).or_insert(0) += 1;
        }

        let has_latency = candidates.contains(&StrategyFamily::LatencyArb);
        let has_spread = candidates.contains(&StrategyFamily::SpreadCapture);
        let has_calm = candidates.contains(&StrategyFamily::CalmPersistence);
        if has_latency && has_spread {
            self.latency_spread_overlap_count += 1;
        }
        if has_latency && has_calm {
            self.latency_calm_overlap_count += 1;
        }
        if has_spread && has_calm {
            self.spread_calm_overlap_count += 1;
        }
    }

    /// Increment the router-block counter.
    pub fn record_router_block(&mut self) {
        self.router_blocked_count += 1;
    }

    /// Record the submission outcome for one selected family in one regime.
    pub fn record_submission(
        &mut self,
        family: StrategyFamily,
        regime: PortfolioRegime,
        outcome: &SubmissionOutcome,
    ) {
        match outcome {
            SubmissionOutcome::Queued { signal_ids } => {
                *self.queued_counts.entry(regime).or_insert(0) += signal_ids.len() as u64;
                for signal_id in signal_ids {
                    self.signal_regimes.insert(*signal_id, regime);
                    self.signal_families.insert(*signal_id, family);
                }
            }
            SubmissionOutcome::Rejected { reason, .. } => {
                if reason == "strategy_sleeve_exhausted" {
                    self.capital_blocked_count += 1;
                }
            }
        }
    }

    /// Attribute processed fills and misses back to their originating regime.
    pub fn record_processed_outcomes(&mut self, outcomes: &[ProcessedOrderOutcome]) {
        for outcome in outcomes {
            let regime = self
                .signal_regimes
                .get(&outcome.signal_id)
                .copied()
                .unwrap_or(PortfolioRegime::Dislocation);
            match outcome.disposition {
                OrderOutcomeDisposition::Filled => {
                    *self.filled_counts.entry(regime).or_insert(0) += 1;
                }
                OrderOutcomeDisposition::Missed => {
                    *self.missed_counts.entry(regime).or_insert(0) += 1;
                }
            }
        }
    }

    #[must_use]
    /// Return how often the provided regime was observed.
    pub fn regime_count(&self, regime: PortfolioRegime) -> u64 {
        self.regime_counts.get(&regime).copied().unwrap_or(0)
    }

    #[must_use]
    /// Return how many candidates were generated for the provided family.
    pub fn candidate_count(&self, family: StrategyFamily) -> u64 {
        self.candidate_counts.get(&family).copied().unwrap_or(0)
    }

    #[must_use]
    /// Return how many signals were queued in the provided regime.
    pub fn queued_count(&self, regime: PortfolioRegime) -> u64 {
        self.queued_counts.get(&regime).copied().unwrap_or(0)
    }

    #[must_use]
    /// Return how many queued orders filled in the provided regime.
    pub fn filled_count(&self, regime: PortfolioRegime) -> u64 {
        self.filled_counts.get(&regime).copied().unwrap_or(0)
    }

    #[must_use]
    /// Return how many queued orders missed in the provided regime.
    pub fn missed_count(&self, regime: PortfolioRegime) -> u64 {
        self.missed_counts.get(&regime).copied().unwrap_or(0)
    }
}

/// Return whether the current snapshot looks like a structural pair opportunity.
#[must_use]
fn looks_like_structural_pair(ctx: &StrategyContext, config: &Config) -> bool {
    let Some(total_ask) = ctx.features.total_ask else {
        return false;
    };
    let Some(up_ask) = ctx.features.up_ask else {
        return false;
    };
    let Some(down_ask) = ctx.features.down_ask else {
        return false;
    };
    let skew_ok = ctx
        .features
        .inter_leg_skew_ms
        .is_none_or(|skew| skew <= config.spread_capture_max_leg_skew_ms);
    let churn_ok = ctx
        .features
        .polymarket_quote_churn_per_s
        .is_none_or(|churn| churn <= config.spread_capture_max_quote_churn_per_s);

    up_ask >= config.spread_capture_min_ask
        && down_ask >= config.spread_capture_min_ask
        && total_ask <= config.spread_capture_threshold
        && skew_ok
        && churn_ok
}

#[must_use]
/// Return whether the current snapshot looks like a calm-market opportunity.
fn looks_like_calm_regime(ctx: &StrategyContext, config: &Config) -> bool {
    if ctx.window_time_remaining_ms < config.calm_persistence_min_window_time_ms
        || ctx.window_time_remaining_ms > config.calm_persistence_max_window_time_ms
    {
        return false;
    }

    let vol_ok = ctx
        .features
        .realized_vol_15s_bps
        .is_some_and(|vol| vol <= config.calm_persistence_max_realized_vol_15s_bps);
    let crosses_ok = ctx
        .features
        .open_crosses_30s
        .is_some_and(|crosses| crosses <= config.calm_persistence_max_open_crosses_30s);
    let churn_ok = ctx
        .features
        .polymarket_quote_churn_per_s
        .is_none_or(|churn| churn <= config.calm_persistence_max_quote_churn_per_s);

    vol_ok && crosses_ok && churn_ok
}

#[cfg(test)]
#[path = "tests/portfolio_tests.rs"]
mod tests;
