use super::*;
use crate::bankroll::BankrollManager;
use crate::clock::BacktestClock;
use crate::config::Config;
use crate::db::database::Database;
use crate::executor::{ExecutionEngine, SubmissionOutcome};
use crate::portfolio::StrategyFamily;
use crate::rejection_diagnostics::StrategyRejectionTracker;
use crate::signal_features::SignalFeatureSnapshot;
use crate::strategies::{Strategy, StrategyResult};
use crate::trend_tracker::ScopedTrendTracker;
use crate::types::{
    BookState, MarketWindow, ReplayFidelity, Signal, SignalDirection, SignalTelemetry,
    SimulatedTrade, StrategyContext, TradeStatus,
};
use tempfile::NamedTempFile;

/// Minimal test strategy that always returns the same single signal.
struct FixedSingleStrategy {
    name: &'static str,
    family: StrategyFamily,
    signal: Signal,
}

impl Strategy for FixedSingleStrategy {
    /// Return the persisted strategy name.
    fn name(&self) -> &'static str {
        self.name
    }

    /// Return the configured family.
    fn family(&self) -> StrategyFamily {
        self.family
    }

    /// Always emit the configured signal.
    fn evaluate(&mut self, _ctx: &StrategyContext, _config: &Config, _now: u64) -> StrategyResult {
        StrategyResult::Single(Box::new(self.signal.clone()))
    }
}

/// Create one temporary SQLite database.
fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

/// Build the standard config for strategy-cycle tests.
fn test_config() -> Config {
    let mut config = Config::default();
    config.latency_arb_enabled = true;
    config.spread_capture_enabled = true;
    config.calm_persistence_enabled = true;
    config.max_position_fraction = 1.0;
    config.max_position_usd_fraction = 1.0;
    config.max_position_usd = 1_000.0;
    config.min_bet_usd = 1.0;
    config.max_open_positions = 10;
    config.sim_order_latency_ms = 250;
    config.latency_arb_max_ask = 0.60;
    config.calm_persistence_max_ask = 0.75;
    config.trend_filter_enabled = false;
    config.trend_filter_per_strategy = true;
    config
}

/// Build one representative market window.
fn test_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-1".to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: "tok-up".to_string(),
        down_token_id: "tok-down".to_string(),
        condition_id: "cond-1".to_string(),
        start_time: 1_000,
        end_time: 301_000,
        slug: "btc-updown-5m".to_string(),
        outcome: None,
        resolution_source: Some("gamma".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(1.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: None,
        taker_base_fee: None,
        rewards_min_size: None,
        rewards_max_spread: None,
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
        token_fee_rates_json: Some("{\"tok-up\":{\"base_fee\":1000}}".to_string()),
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    }
}

/// Build one empty strategy context.
fn test_ctx() -> StrategyContext {
    StrategyContext {
        binance_price: 68_000.0,
        binance_momentum: 0.0,
        chainlink_price: Some(68_000.0),
        book_state: BookState::default(),
        window_open_price: Some(68_000.0),
        window_time_remaining_ms: 60_000,
        now_us: Some(1_000_000),
        features: SignalFeatureSnapshot::default(),
    }
}

/// Build one telemetry-bearing single-side signal.
fn single_signal(strategy: &str, side: SignalDirection, up_ask: f64, down_ask: f64) -> Signal {
    Signal {
        timestamp: 80_000,
        strategy: strategy.to_string(),
        strategy_version: "v1".to_string(),
        feature_mode: "raw_event".to_string(),
        direction: side,
        confidence: 1.0,
        binance_price: 68_000.0,
        chainlink_price: 68_000.0,
        up_ask,
        down_ask,
        up_bid: (up_ask - 0.01).max(0.0),
        down_bid: (down_ask - 0.01).max(0.0),
        expected_edge: Some(0.12),
        metadata: serde_json::json!({"expectedEdge": 0.12}),
        telemetry: Some(SignalTelemetry {
            generated_at_ms: 80_000,
            generated_at_us: None,
            order_submitted_at_ms: None,
            order_submitted_at_us: None,
            expected_arrival_at_ms: None,
            expected_arrival_at_us: None,
            order_processed_at_ms: None,
            order_processed_at_us: None,
            effective_arrival_delay_ms: None,
            binance_age_ms: Some(0),
            chainlink_age_ms: Some(0),
            clob_age_ms: Some(0),
            quote_age_ms: Some(0),
            book_staleness_ms: Some(0),
            expected_fee: Some(0.01),
            expected_slippage: Some(0.01),
            expected_edge: Some(0.12),
            available_feature_count: 1,
            decision_status: "generated".to_string(),
            rejection_reason: None,
            features_json: serde_json::json!({"test": true}),
        }),
    }
}

/// Count persisted signals.
fn signal_count(db: &Database) -> i64 {
    db.conn()
        .query_row("SELECT COUNT(*) FROM signals", [], |row| row.get(0))
        .unwrap()
}

/// Count persisted signal metrics.
fn signal_metric_count(db: &Database) -> i64 {
    db.conn()
        .query_row("SELECT COUNT(*) FROM signal_metrics", [], |row| row.get(0))
        .unwrap()
}

/// Count current rejection summaries in the tracker.
fn rejection_count_for_reason(
    tracker: &StrategyRejectionTracker,
    market_id: &str,
    strategy: &str,
    reason: &str,
) -> u64 {
    tracker
        .snapshot_all(90_000)
        .into_iter()
        .find(|row| row.market_id == market_id && row.strategy == strategy && row.reason == reason)
        .map_or(0, |row| row.count)
}

/// Verify calm duplicate-pending attempts are rejected before signal persistence.
#[test]
fn calm_duplicate_pending_order_is_blocked_before_signal_persistence() {
    let (db, _tmp) = temp_db();
    let config = test_config();
    let clock = BacktestClock::new();
    let window = test_window();
    db.upsert_market(&window).unwrap();

    let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
    let mut engine = ExecutionEngine::new();
    let mut trend_tracker = ScopedTrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
        config.trend_filter_per_strategy,
    );
    let mut rejection_tracker = StrategyRejectionTracker::new();

    let signal = single_signal("calm-persistence", SignalDirection::Up, 0.72, 0.28);
    let queued = engine
        .submit_single(
            &signal,
            &window,
            &db,
            &mut bankroll,
            &config,
            &clock,
            ReplayFidelity::RawEvent,
        )
        .unwrap();
    assert!(matches!(queued, SubmissionOutcome::Queued { .. }));
    assert_eq!(signal_count(&db), 1);
    assert_eq!(signal_metric_count(&db), 1);

    let mut strategies: Vec<Box<dyn Strategy>> = vec![Box::new(FixedSingleStrategy {
        name: "calm-persistence",
        family: StrategyFamily::CalmPersistence,
        signal,
    })];

    let cycle = run_strategy_cycle(
        &test_ctx(),
        &window,
        &config,
        &clock,
        &db,
        &mut strategies,
        &mut engine,
        &mut bankroll,
        &mut trend_tracker,
        &mut rejection_tracker,
        ReplayFidelity::RawEvent,
        80_000,
    )
    .unwrap();

    assert_eq!(cycle.signal_count_delta, 0);
    assert!(cycle.submissions.is_empty());
    assert_eq!(signal_count(&db), 1);
    assert_eq!(signal_metric_count(&db), 1);
    assert_eq!(
        rejection_count_for_reason(
            &rejection_tracker,
            &window.market_id,
            "calm-persistence",
            "duplicate_pending_order",
        ),
        1
    );
}

/// Verify calm duplicate-open attempts are rejected before signal persistence.
#[test]
fn calm_duplicate_open_position_is_blocked_before_signal_persistence() {
    let (db, _tmp) = temp_db();
    let config = test_config();
    let clock = BacktestClock::new();
    let window = test_window();
    db.upsert_market(&window).unwrap();

    db.open_trade(&SimulatedTrade {
        id: None,
        timestamp: 79_000,
        market_id: window.market_id.clone(),
        strategy: "calm-persistence".to_string(),
        side: SignalDirection::Up,
        token_id: window.up_token_id.clone(),
        entry_price: 0.55,
        size: 10.0,
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
    })
    .unwrap();

    let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
    let mut engine = ExecutionEngine::new();
    let mut trend_tracker = ScopedTrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
        config.trend_filter_per_strategy,
    );
    let mut rejection_tracker = StrategyRejectionTracker::new();
    let signal = single_signal("calm-persistence", SignalDirection::Up, 0.72, 0.28);
    let mut strategies: Vec<Box<dyn Strategy>> = vec![Box::new(FixedSingleStrategy {
        name: "calm-persistence",
        family: StrategyFamily::CalmPersistence,
        signal,
    })];

    let cycle = run_strategy_cycle(
        &test_ctx(),
        &window,
        &config,
        &clock,
        &db,
        &mut strategies,
        &mut engine,
        &mut bankroll,
        &mut trend_tracker,
        &mut rejection_tracker,
        ReplayFidelity::RawEvent,
        80_000,
    )
    .unwrap();

    assert_eq!(cycle.signal_count_delta, 0);
    assert!(cycle.submissions.is_empty());
    assert_eq!(signal_count(&db), 0);
    assert_eq!(signal_metric_count(&db), 0);
    assert_eq!(
        rejection_count_for_reason(
            &rejection_tracker,
            &window.market_id,
            "calm-persistence",
            "duplicate_open_position",
        ),
        1
    );
}

/// Verify latency duplicates still persist signals and reject at the executor layer.
#[test]
fn latency_duplicate_open_position_still_persists_signal() {
    let (db, _tmp) = temp_db();
    let config = test_config();
    let clock = BacktestClock::new();
    let window = test_window();
    db.upsert_market(&window).unwrap();

    db.open_trade(&SimulatedTrade {
        id: None,
        timestamp: 79_000,
        market_id: window.market_id.clone(),
        strategy: "latency-arb".to_string(),
        side: SignalDirection::Up,
        token_id: window.up_token_id.clone(),
        entry_price: 0.55,
        size: 10.0,
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
    })
    .unwrap();

    let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
    let mut engine = ExecutionEngine::new();
    let mut trend_tracker = ScopedTrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
        config.trend_filter_per_strategy,
    );
    let mut rejection_tracker = StrategyRejectionTracker::new();
    let mut strategies: Vec<Box<dyn Strategy>> = vec![Box::new(FixedSingleStrategy {
        name: "latency-arb",
        family: StrategyFamily::LatencyArb,
        signal: single_signal("latency-arb", SignalDirection::Up, 0.55, 0.45),
    })];

    let cycle = run_strategy_cycle(
        &test_ctx(),
        &window,
        &config,
        &clock,
        &db,
        &mut strategies,
        &mut engine,
        &mut bankroll,
        &mut trend_tracker,
        &mut rejection_tracker,
        ReplayFidelity::RawEvent,
        80_000,
    )
    .unwrap();

    assert_eq!(cycle.signal_count_delta, 1);
    assert_eq!(signal_count(&db), 1);
    assert_eq!(signal_metric_count(&db), 1);
    let metric: (String, String) = db
        .conn()
        .query_row(
            "SELECT decision_status, rejection_reason FROM signal_metrics LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(metric.0, "rejected");
    assert_eq!(metric.1, "duplicate_open_position");
    assert!(rejection_tracker.snapshot_all(90_000).is_empty());
}
