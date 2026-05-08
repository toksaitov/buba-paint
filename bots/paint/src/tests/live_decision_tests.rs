use super::*;

use crate::config::{Config, ExecutionMode};
use crate::portfolio::StrategyFamily;
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{BookState, MarketWindow, Signal, SignalDirection, StrategyContext, TopOfBook};

/// Deterministic strategy used to exercise the pure decision worker.
struct FixedSignalStrategy {
    strategy: &'static str,
    family: StrategyFamily,
    side: SignalDirection,
}

impl Strategy for FixedSignalStrategy {
    /// Returns the stable test strategy name.
    fn name(&self) -> &'static str {
        self.strategy
    }

    /// Returns the stable test strategy family.
    fn family(&self) -> StrategyFamily {
        self.family
    }

    /// Emits one deterministic signal on every evaluation.
    fn evaluate(&mut self, ctx: &StrategyContext, _config: &Config, now: u64) -> StrategyResult {
        StrategyResult::Single(Box::new(Signal {
            timestamp: now,
            strategy: self.strategy.to_string(),
            strategy_version: "test".to_string(),
            feature_mode: "raw_event_full".to_string(),
            direction: self.side,
            confidence: 1.0,
            binance_price: ctx.binance_price,
            chainlink_price: ctx.chainlink_price.unwrap_or(ctx.binance_price),
            up_ask: 0.50,
            down_ask: 0.50,
            up_bid: 0.49,
            down_bid: 0.49,
            expected_edge: Some(0.05),
            metadata: serde_json::json!({"testSignal": true}),
            telemetry: None,
        }))
    }
}

/// Build one low-friction runtime config for decision-worker tests.
fn test_config(execution_mode: ExecutionMode) -> Config {
    let mut config = Config::default();
    config.execution_mode = execution_mode;
    config.regime_detection_enabled = false;
    config.min_bet_usd = 1.0;
    config.max_open_positions = 5;
    config.sim_order_latency_ms = 25;
    config
}

/// Build one market window with enough metadata for live-order conversion.
fn test_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-test".to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: "up-token".to_string(),
        down_token_id: "down-token".to_string(),
        condition_id: "condition".to_string(),
        start_time: 1_000,
        end_time: 301_000,
        slug: "btc-test".to_string(),
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
        token_fee_rates_json: None,
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2026-05-08T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    }
}

/// Build one liquid top-of-book snapshot.
fn top_of_book(best_ask: f64, now_ms: u64) -> TopOfBook {
    TopOfBook {
        best_bid: (best_ask - 0.01).max(0.0),
        best_ask,
        bid_size: 500.0,
        ask_size: 500.0,
        timestamp: now_ms,
        observed_at_ms: now_ms,
    }
}

/// Build one strategy context with replay-grade feature placeholders.
fn test_context(now_ms: u64) -> StrategyContext {
    let book_state = BookState {
        up: Some(top_of_book(0.50, now_ms)),
        down: Some(top_of_book(0.50, now_ms)),
    };
    StrategyContext {
        binance_price: 75_000.0,
        binance_momentum: 0.01,
        chainlink_price: Some(75_000.0),
        book_state,
        window_open_price: Some(74_900.0),
        window_time_remaining_ms: 120_000,
        now_us: Some(now_ms * 1_000),
        features: crate::signal_features::SignalFeatureSnapshot::default(),
    }
}

/// Build one pure decision request for a given window and timestamp.
fn decision_request(
    window: MarketWindow,
    now_ms: u64,
    live_trading_can_submit: bool,
) -> RuntimeDecisionRequest {
    let ctx = test_context(now_ms);
    RuntimeDecisionRequest {
        book_state: ctx.book_state.clone(),
        now_us: ctx.now_us,
        ctx,
        window,
        now_ms,
        live_trading_can_submit,
    }
}

/// Build one decision engine seeded with empty in-memory exposure.
fn engine(execution_mode: ExecutionMode) -> RuntimeDecisionEngine {
    RuntimeDecisionEngine::new(
        test_config(execution_mode),
        vec![Box::new(FixedSignalStrategy {
            strategy: "latency-arb",
            family: StrategyFamily::LatencyArb,
            side: SignalDirection::Up,
        })],
        RuntimeDecisionSeed {
            starting_balance: 100.0,
            current_balance: 100.0,
            unresolved_exposures: Vec::new(),
            open_trades: Vec::new(),
            now_ms: 1_000,
        },
    )
}

#[test]
/// Verifies paper decisions emit async persistence work without live orders.
fn paper_decision_emits_persistence_without_database() {
    let mut engine = engine(ExecutionMode::Paper);
    let output = engine.evaluate(decision_request(test_window(), 10_000, true));

    assert!(output.live_orders.is_empty());
    assert!(
        output
            .persistence_events
            .iter()
            .any(|event| matches!(event, LivePersistenceEvent::Signal { .. }))
    );
    assert!(
        output
            .log_events
            .iter()
            .any(|event| matches!(event, RuntimeDecisionLogEvent::SingleSubmitted { .. }))
    );
}

#[test]
/// Verifies pending live intents block duplicate submissions before DB feedback.
fn live_decision_blocks_duplicate_pending_intent_in_memory() {
    let window = test_window();
    let mut engine = engine(ExecutionMode::LiveTrading);
    let first = engine.evaluate(decision_request(window.clone(), 10_000, true));
    assert_eq!(first.live_orders.len(), 1);

    let second = engine.evaluate(decision_request(window, 10_001, true));
    assert!(second.live_orders.is_empty());
    assert!(second.persistence_events.iter().any(|event| matches!(
        event,
        LivePersistenceEvent::Signal {
            decision_status,
            rejection_reason,
            ..
        } if decision_status == "rejected"
            && rejection_reason.as_deref() == Some("duplicate_pending_order")
    )));
}

#[test]
/// Verifies filled live feedback becomes in-memory exposure for duplicate checks.
fn live_submission_feedback_converts_pending_intent_to_open_exposure() {
    let window = test_window();
    let mut engine = engine(ExecutionMode::LiveTrading);
    let first = engine.evaluate(decision_request(window.clone(), 10_000, true));
    let signal_id = first.live_orders[0].signal_id;

    engine.apply_live_submission_feedback(&[signal_id], &[], 10_050);

    let second = engine.evaluate(decision_request(window, 10_060, true));
    assert!(second.live_orders.is_empty());
    assert!(second.persistence_events.iter().any(|event| matches!(
        event,
        LivePersistenceEvent::Signal {
            decision_status,
            rejection_reason,
            ..
        } if decision_status == "rejected"
            && rejection_reason.as_deref() == Some("duplicate_open_position")
    )));
}
