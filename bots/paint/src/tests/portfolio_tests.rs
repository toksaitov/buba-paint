use super::*;
use crate::config::Config;
use crate::executor::{OrderOutcomeDisposition, ProcessedOrderOutcome, SubmissionOutcome};
use crate::signal_features::SignalFeatureSnapshot;
use crate::types::{BookState, StrategyContext};

/// Build one minimal strategy context.
fn ctx(features: SignalFeatureSnapshot, remaining_ms: u64) -> StrategyContext {
    StrategyContext {
        binance_price: 100.0,
        binance_momentum: 0.0,
        chainlink_price: Some(100.0),
        book_state: BookState::default(),
        window_open_price: Some(100.0),
        window_time_remaining_ms: remaining_ms,
        now_us: Some(1_000_000),
        features,
    }
}

/// Verify that calm-looking snapshots classify as calm.
#[test]
fn calm_snapshot_routes_to_calm_family() {
    let config = Config {
        calm_persistence_enabled: true,
        calm_persistence_max_window_time_ms: 90_000,
        calm_persistence_min_window_time_ms: 30_000,
        calm_persistence_max_realized_vol_15s_bps: 35.0,
        calm_persistence_max_open_crosses_30s: 1,
        calm_persistence_max_quote_churn_per_s: 20.0,
        ..Config::default()
    };
    let features = SignalFeatureSnapshot {
        realized_vol_15s_bps: Some(10.0),
        open_crosses_30s: Some(0),
        polymarket_quote_churn_per_s: Some(5.0),
        ..SignalFeatureSnapshot::default()
    };

    let observed = detect_regime(&ctx(features.clone(), 60_000), &config);
    assert_eq!(observed, PortfolioRegime::Calm);

    let selection = select_family_for_candidates(
        &ctx(features, 60_000),
        &config,
        &[StrategyFamily::LatencyArb, StrategyFamily::CalmPersistence],
    );
    assert_eq!(
        selection.selected_family,
        Some(StrategyFamily::CalmPersistence)
    );
}

/// Verify that structural-pair snapshots route to spread-capture.
#[test]
fn structural_pair_snapshot_routes_to_spread_family() {
    let config = Config {
        spread_capture_threshold: 0.97,
        spread_capture_min_ask: 0.15,
        spread_capture_max_leg_skew_ms: 25,
        spread_capture_max_quote_churn_per_s: 8.0,
        ..Config::default()
    };
    let features = SignalFeatureSnapshot {
        up_ask: Some(0.45),
        down_ask: Some(0.46),
        total_ask: Some(0.91),
        inter_leg_skew_ms: Some(10),
        polymarket_quote_churn_per_s: Some(4.0),
        ..SignalFeatureSnapshot::default()
    };

    let observed = detect_regime(&ctx(features.clone(), 120_000), &config);
    assert_eq!(observed, PortfolioRegime::StructuralPair);

    let selection = select_family_for_candidates(
        &ctx(features, 120_000),
        &config,
        &[StrategyFamily::SpreadCapture, StrategyFamily::LatencyArb],
    );
    assert_eq!(
        selection.selected_family,
        Some(StrategyFamily::SpreadCapture)
    );
}

/// Verify that dislocation is the fallback regime when calm and spread conditions do not hold.
#[test]
fn dislocation_is_the_default_fallback_regime() {
    let config = Config::default();
    let features = SignalFeatureSnapshot {
        realized_vol_15s_bps: Some(100.0),
        open_crosses_30s: Some(8),
        polymarket_quote_churn_per_s: Some(50.0),
        up_ask: Some(0.70),
        down_ask: Some(0.40),
        total_ask: Some(1.10),
        ..SignalFeatureSnapshot::default()
    };

    let observed = detect_regime(&ctx(features.clone(), 150_000), &config);
    assert_eq!(observed, PortfolioRegime::Dislocation);

    let selection = select_family_for_candidates(
        &ctx(features, 150_000),
        &config,
        &[StrategyFamily::LatencyArb, StrategyFamily::CalmPersistence],
    );
    assert_eq!(selection.selected_family, Some(StrategyFamily::LatencyArb));
}

/// Verify that non-matching candidates are blocked instead of falling through.
#[test]
fn router_blocks_when_no_candidate_matches_regime() {
    let config = Config {
        spread_capture_threshold: 0.97,
        spread_capture_min_ask: 0.15,
        spread_capture_max_leg_skew_ms: 25,
        spread_capture_max_quote_churn_per_s: 8.0,
        ..Config::default()
    };
    let features = SignalFeatureSnapshot {
        up_ask: Some(0.45),
        down_ask: Some(0.46),
        total_ask: Some(0.91),
        inter_leg_skew_ms: Some(10),
        polymarket_quote_churn_per_s: Some(4.0),
        ..SignalFeatureSnapshot::default()
    };

    let selection = select_family_for_candidates(
        &ctx(features, 120_000),
        &config,
        &[StrategyFamily::LatencyArb],
    );
    assert_eq!(selection.regime, PortfolioRegime::StructuralPair);
    assert_eq!(selection.selected_family, None);
}

/// Verify overlap, router-block, and capital-block attribution accounting.
#[test]
fn portfolio_stats_track_overlap_and_blocked_counts() {
    let mut stats = PortfolioAttributionStats::default();
    stats.record_candidates(&[
        StrategyFamily::LatencyArb,
        StrategyFamily::SpreadCapture,
        StrategyFamily::CalmPersistence,
    ]);
    stats.record_router_block();
    stats.record_submission(
        StrategyFamily::CalmPersistence,
        PortfolioRegime::Calm,
        &SubmissionOutcome::Rejected {
            signal_ids: vec![],
            reason: "strategy_sleeve_exhausted".to_string(),
        },
    );
    stats.record_submission(
        StrategyFamily::LatencyArb,
        PortfolioRegime::Dislocation,
        &SubmissionOutcome::Queued {
            signal_ids: vec![7],
        },
    );
    stats.record_processed_outcomes(&[ProcessedOrderOutcome {
        signal_id: 7,
        strategy: "latency-arb".to_string(),
        market_id: "mkt-1".to_string(),
        side: crate::types::SignalDirection::Up,
        disposition: OrderOutcomeDisposition::Filled,
        reason: None,
        best_ask: Some(0.5),
        ask_size: Some(10.0),
        freshness_ms: Some(1),
        requested_size: 1.0,
        filled_size: 1.0,
        effective_arrival_delay_ms: 250,
        partial_fill: false,
    }]);

    assert_eq!(stats.latency_spread_overlap_count, 1);
    assert_eq!(stats.latency_calm_overlap_count, 1);
    assert_eq!(stats.spread_calm_overlap_count, 1);
    assert_eq!(stats.router_blocked_count, 1);
    assert_eq!(stats.capital_blocked_count, 1);
    assert_eq!(stats.queued_count(PortfolioRegime::Dislocation), 1);
    assert_eq!(stats.filled_count(PortfolioRegime::Dislocation), 1);
}
