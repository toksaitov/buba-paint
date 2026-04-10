use crate::bankroll::{BankrollManager, SpreadAffordabilityCheck};
use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::Database;
use crate::executor::{ExecutionEngine, QueueRejectionReason, SubmissionOutcome};
use crate::portfolio::{
    PortfolioRegime, StrategyFamily, detect_regime, select_family_for_candidates,
};
use crate::rejection_diagnostics::StrategyRejectionTracker;
use crate::strategies::{Strategy, StrategyResult};
use crate::trend_tracker::ScopedTrendTracker;
use crate::types::{
    MarketWindow, ReplayFidelity, Signal, SignalDirection, StrategyContext, StrategyRejection,
    StrategyRejectionReason, StrategyRejectionSample,
};

/// One evaluated strategy candidate together with its selected portfolio family.
struct EvaluatedCandidate {
    family: StrategyFamily,
    result: StrategyResult,
}

/// One caller-visible event emitted by the shared strategy cycle.
#[derive(Debug, Clone)]
pub enum StrategyCycleEvent {
    Suppressed {
        strategy: String,
        direction: SignalDirection,
        regime: PortfolioRegime,
    },
    SingleSubmitted {
        strategy: String,
        direction: SignalDirection,
        regime: PortfolioRegime,
        outcome: SubmissionOutcome,
    },
    BatchSubmitted {
        strategy: String,
        count: usize,
        regime: PortfolioRegime,
        outcome: SubmissionOutcome,
    },
}

/// One portfolio-attribution record emitted by the shared strategy cycle.
#[derive(Debug, Clone)]
pub struct StrategyCycleSubmission {
    pub family: StrategyFamily,
    pub regime: PortfolioRegime,
    pub outcome: SubmissionOutcome,
}

/// Aggregate output from one shared evaluation and submission cycle.
#[derive(Debug, Clone)]
pub struct StrategyCycleResult {
    pub observed_regime: PortfolioRegime,
    pub candidate_families: Vec<StrategyFamily>,
    pub router_blocked_count: u64,
    pub signal_count_delta: u64,
    pub submissions: Vec<StrategyCycleSubmission>,
    pub events: Vec<StrategyCycleEvent>,
}

/// Run one full strategy-evaluation and submission cycle for one market snapshot.
#[allow(clippy::too_many_arguments)]
pub fn run_strategy_cycle(
    ctx: &StrategyContext,
    window: &MarketWindow,
    config: &Config,
    clock: &dyn Clock,
    db: &Database,
    strategies: &mut [Box<dyn Strategy>],
    execution_engine: &mut ExecutionEngine,
    bankroll: &mut BankrollManager,
    trend_tracker: &mut ScopedTrendTracker,
    rejection_tracker: &mut StrategyRejectionTracker,
    execution_fidelity: ReplayFidelity,
    now: u64,
) -> anyhow::Result<StrategyCycleResult> {
    let observed_regime = detect_regime(ctx, config);
    let candidates = collect_candidates(
        ctx,
        config,
        now,
        strategies,
        rejection_tracker,
        &window.market_id,
    );

    let candidate_families = candidates
        .iter()
        .map(|candidate| candidate.family)
        .collect::<Vec<_>>();
    let selected_family = if config.regime_detection_enabled {
        select_family_for_candidates(ctx, config, &candidate_families).selected_family
    } else {
        None
    };

    let mut result = StrategyCycleResult {
        observed_regime,
        candidate_families,
        router_blocked_count: 0,
        signal_count_delta: 0,
        submissions: Vec::new(),
        events: Vec::new(),
    };

    for candidate in candidates {
        if config.regime_detection_enabled && Some(candidate.family) != selected_family {
            rejection_tracker.record(
                &window.market_id,
                &StrategyRejection::new(
                    candidate.family.as_str(),
                    StrategyRejectionReason::BlockedByRouter,
                    StrategyRejectionSample::from_ctx(ctx),
                ),
            );
            result.router_blocked_count += 1;
            continue;
        }

        match candidate.result {
            StrategyResult::Single(signal) => handle_single_candidate(
                ctx,
                *signal,
                candidate.family,
                observed_regime,
                window,
                db,
                bankroll,
                trend_tracker,
                execution_engine,
                rejection_tracker,
                config,
                clock,
                execution_fidelity,
                &mut result,
            )?,
            StrategyResult::Batch(signals) => handle_batch_candidate(
                signals,
                candidate.family,
                observed_regime,
                ctx,
                window,
                db,
                bankroll,
                execution_engine,
                rejection_tracker,
                config,
                clock,
                execution_fidelity,
                &mut result,
            )?,
            StrategyResult::None | StrategyResult::Rejected(_) => {}
        }
    }

    Ok(result)
}

/// Evaluate every strategy and keep only actionable candidates.
fn collect_candidates(
    ctx: &StrategyContext,
    config: &Config,
    now: u64,
    strategies: &mut [Box<dyn Strategy>],
    rejection_tracker: &mut StrategyRejectionTracker,
    market_id: &str,
) -> Vec<EvaluatedCandidate> {
    let mut candidates = Vec::new();
    for strategy in strategies.iter_mut() {
        let result = strategy.evaluate(ctx, config, now);
        match result {
            StrategyResult::None => {}
            StrategyResult::Rejected(rejection) => rejection_tracker.record(market_id, &rejection),
            StrategyResult::Single(_) | StrategyResult::Batch(_) => {
                candidates.push(EvaluatedCandidate {
                    family: strategy.family(),
                    result,
                });
            }
        }
    }
    candidates
}

/// Handle one single-side candidate emitted by a strategy.
#[allow(clippy::too_many_arguments)]
fn handle_single_candidate(
    ctx: &StrategyContext,
    mut signal: Signal,
    family: StrategyFamily,
    regime: PortfolioRegime,
    window: &MarketWindow,
    db: &Database,
    bankroll: &mut BankrollManager,
    trend_tracker: &mut ScopedTrendTracker,
    execution_engine: &mut ExecutionEngine,
    rejection_tracker: &mut StrategyRejectionTracker,
    config: &Config,
    clock: &dyn Clock,
    execution_fidelity: ReplayFidelity,
    result: &mut StrategyCycleResult,
) -> anyhow::Result<()> {
    annotate_signal(&mut signal, regime, family);
    if trend_tracker.should_suppress(family, signal.direction) {
        persist_suppressed_signal(&signal, window, db, execution_fidelity);
        result.signal_count_delta += 1;
        result.events.push(StrategyCycleEvent::Suppressed {
            strategy: signal.strategy,
            direction: signal.direction,
            regime,
        });
        return Ok(());
    }
    if family == StrategyFamily::CalmPersistence
        && let Some(reason) =
            execution_engine.precheck_single_submission(&signal, window, db, config, clock)?
        && let Some(rejection_reason) = calm_duplicate_rejection_reason(reason)
    {
        rejection_tracker.record(
            &window.market_id,
            &StrategyRejection::new(
                signal.strategy.as_str(),
                rejection_reason,
                calm_duplicate_rejection_sample(ctx, &signal),
            ),
        );
        return Ok(());
    }

    let submission = execution_engine.submit_single(
        &signal,
        window,
        db,
        bankroll,
        config,
        clock,
        execution_fidelity,
    )?;
    record_submission(result, family, regime, submission.clone());
    result.events.push(StrategyCycleEvent::SingleSubmitted {
        strategy: signal.strategy,
        direction: signal.direction,
        regime,
        outcome: submission,
    });
    Ok(())
}

/// Map queue-layer duplicate reasons into strategy-layer calm diagnostics.
fn calm_duplicate_rejection_reason(
    reason: QueueRejectionReason,
) -> Option<StrategyRejectionReason> {
    match reason {
        QueueRejectionReason::DuplicateOpenPosition => {
            Some(StrategyRejectionReason::DuplicateOpenPosition)
        }
        QueueRejectionReason::DuplicatePendingOrder => {
            Some(StrategyRejectionReason::DuplicatePendingOrder)
        }
        QueueRejectionReason::MaxOpenPositions => None,
    }
}

/// Build one calm duplicate rejection sample without mutating DB state.
fn calm_duplicate_rejection_sample(
    ctx: &StrategyContext,
    signal: &Signal,
) -> StrategyRejectionSample {
    let mut sample = StrategyRejectionSample::from_ctx(ctx);
    sample.up_ask = Some(signal.up_ask);
    sample.down_ask = Some(signal.down_ask);
    sample.expected_edge = signal.expected_edge;
    sample
}

/// Handle one spread batch emitted by a strategy.
#[allow(clippy::too_many_arguments)]
fn handle_batch_candidate(
    mut signals: Vec<Signal>,
    family: StrategyFamily,
    regime: PortfolioRegime,
    ctx: &StrategyContext,
    window: &MarketWindow,
    db: &Database,
    bankroll: &mut BankrollManager,
    execution_engine: &mut ExecutionEngine,
    rejection_tracker: &mut StrategyRejectionTracker,
    config: &Config,
    clock: &dyn Clock,
    execution_fidelity: ReplayFidelity,
    result: &mut StrategyCycleResult,
) -> anyhow::Result<()> {
    for signal in &mut signals {
        annotate_signal(signal, regime, family);
    }
    let strategy_name = signals
        .first()
        .map_or_else(|| "?".to_string(), |signal| signal.strategy.clone());
    if let Some(affordability) =
        assess_spread_batch_affordability(&signals, window, bankroll, config)
        && !affordability.queueable
    {
        rejection_tracker.record(
            &window.market_id,
            &StrategyRejection::new(
                strategy_name.as_str(),
                StrategyRejectionReason::SpreadBudgetTooSmall,
                spread_budget_rejection_sample(ctx, &signals, affordability),
            ),
        );
        return Ok(());
    }

    let submission = execution_engine.submit_spread(
        &signals,
        window,
        db,
        bankroll,
        config,
        clock,
        execution_fidelity,
    )?;
    record_submission(result, family, regime, submission.clone());
    result.events.push(StrategyCycleEvent::BatchSubmitted {
        strategy: strategy_name,
        count: signals.len(),
        regime,
        outcome: submission,
    });
    Ok(())
}

/// Persist one signal that was suppressed by the trend filter.
fn persist_suppressed_signal(
    signal: &Signal,
    window: &MarketWindow,
    db: &Database,
    execution_fidelity: ReplayFidelity,
) {
    if let Ok(signal_id) = db.log_signal_with_context(
        signal,
        Some(&window.market_id),
        Some(execution_fidelity),
        None,
        None,
    ) && let Some(telemetry) = signal.telemetry.as_ref()
    {
        let _ = db.upsert_signal_telemetry(
            signal_id,
            telemetry,
            None,
            None,
            Some("suppressed"),
            Some("trend_filter"),
        );
    }
}

/// Record one submission in the aggregate cycle result.
fn record_submission(
    result: &mut StrategyCycleResult,
    family: StrategyFamily,
    regime: PortfolioRegime,
    outcome: SubmissionOutcome,
) {
    result.signal_count_delta += submission_signal_count(&outcome);
    result.submissions.push(StrategyCycleSubmission {
        family,
        regime,
        outcome,
    });
}

/// Infer the portfolio family from one persisted strategy name.
pub fn family_for_strategy(strategy: &str) -> StrategyFamily {
    StrategyFamily::from_strategy_name(strategy).unwrap_or(StrategyFamily::LatencyArb)
}

/// Attach portfolio-routing metadata to one emitted signal.
pub fn annotate_signal(signal: &mut Signal, regime: PortfolioRegime, family: StrategyFamily) {
    let metadata = signal.metadata.take();
    signal.metadata = serde_json::json!({
        "portfolioRegime": regime.as_str(),
        "strategyFamily": family.as_str(),
        "strategyMetadata": metadata,
    });
}

/// Build the current non-mutating affordability assessment for one spread batch.
pub fn assess_spread_batch_affordability(
    signals: &[Signal],
    window: &MarketWindow,
    bankroll: &BankrollManager,
    config: &Config,
) -> Option<SpreadAffordabilityCheck> {
    let (up_signal, down_signal) = spread_signal_pair(signals)?;
    Some(bankroll.assess_spread_affordability(
        up_signal.up_ask,
        down_signal.down_ask,
        window.order_min_size.unwrap_or(0.0),
        config,
    ))
}

/// Build the structured rejection sample persisted for an impossible spread batch.
pub fn spread_budget_rejection_sample(
    ctx: &StrategyContext,
    signals: &[Signal],
    affordability: SpreadAffordabilityCheck,
) -> StrategyRejectionSample {
    let mut sample = StrategyRejectionSample::from_ctx(ctx);
    if let Some((up_signal, down_signal)) = spread_signal_pair(signals) {
        sample.up_ask = Some(up_signal.up_ask);
        sample.down_ask = Some(down_signal.down_ask);
        sample.total_ask = Some(up_signal.up_ask + down_signal.down_ask);
    }
    sample.available_spread_budget = Some(affordability.available_spread_budget);
    sample.required_pair_notional = Some(affordability.required_pair_notional);
    sample.required_pair_units = Some(affordability.required_pair_units);
    sample
}

/// Return the persisted signal count implied by one submission outcome.
fn submission_signal_count(outcome: &SubmissionOutcome) -> u64 {
    match outcome {
        SubmissionOutcome::Queued { signal_ids }
        | SubmissionOutcome::Rejected { signal_ids, .. } => signal_ids.len() as u64,
    }
}

/// Return the up/down leg pair from one spread batch when both sides are present.
fn spread_signal_pair(signals: &[Signal]) -> Option<(&Signal, &Signal)> {
    let up_signal = signals
        .iter()
        .find(|signal| signal.direction == SignalDirection::Up)?;
    let down_signal = signals
        .iter()
        .find(|signal| signal.direction == SignalDirection::Down)?;
    Some((up_signal, down_signal))
}

#[cfg(test)]
#[path = "tests/strategy_cycle_tests.rs"]
mod tests;
