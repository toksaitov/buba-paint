use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use serde_json::json;
use tracing::{error, info, warn};

use crate::backtest::momentum::MomentumCalculator;
use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::{Clock, SystemClock};
use crate::config::{Config, FeedEventStorageProfile};
use crate::db::database::{Database, FeedEventFootprintRow};
use crate::executor::{
    ExecutionEngine, OrderOutcomeDisposition, ProcessedOrderOutcome, QueuedOrderIntent,
    SubmissionOutcome,
};
use crate::feeds::FeedMessage;
use crate::feeds::util::now_us;
use crate::live_control::{LiveControlAction, record_live_control_state};
use crate::live_sidecar::{
    LiveAccountState, LiveActivityResponse, LiveCheckStatus, LiveOrderIntentRequest,
    LiveOrderIntentResponse, LivePreflightResponse, LiveSidecarClient,
};
use crate::live_storage::FeedEventStorageState;
use crate::market_discovery::{self, MarketDiscoveryEvent};
use crate::position_manager::PositionManager;
use crate::rejection_diagnostics::StrategyRejectionTracker;
use crate::signal_features::{SignalFeatureEngine, SignalState};
use crate::strategies::Strategy;
use crate::strategies::calm_persistence::CalmPersistenceStrategy;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategy_cycle::{StrategyCycleEvent, family_for_strategy, run_strategy_cycle};
use crate::tick_logger::{self, TickLoggerState};
use crate::trend_tracker::ScopedTrendTracker;
use crate::types::{
    FeedEvent, FeedHealthEvent, LiveAccountSnapshot, LiveFill, LiveOrder, LiveOrderIntent,
    LiveReconciliationEvent, LiveRedemption, LiveSession, MarketWindow, ReplayFidelity,
    SignalDirection, StrategyContext,
};

struct PendingResolution {
    market_id: String,
    window: MarketWindow,
    next_attempt_at_ms: u64,
    seeded_from_startup: bool,
}

struct LiveState {
    signal_state: SignalState,
    current_window: Option<MarketWindow>,
    window_open_prices: std::collections::HashMap<String, f64>,
    known_windows: std::collections::HashMap<String, MarketWindow>,
}

struct LiveTradingRuntimeBootstrap {
    live_starting_balance: f64,
    monitor: LiveTradingMonitor,
}

struct LiveTradingMonitor {
    sidecar: LiveSidecarClient,
    session_id: i64,
    state: String,
    preflight: Option<LivePreflightResponse>,
    account: Option<LiveAccountState>,
    activity: Option<LiveActivityResponse>,
    blocked_reason: Option<String>,
    finished: bool,
}

/// Build the enabled live strategy list for the current configuration.
fn build_strategies(config: &Config) -> Vec<Box<dyn Strategy>> {
    let mut strategies: Vec<Box<dyn Strategy>> = Vec::new();
    if config.latency_arb_enabled {
        strategies.push(Box::new(LatencyArbStrategy::new(
            config.latency_arb_momentum_threshold,
        )));
    }
    if config.spread_capture_enabled {
        strategies.push(Box::new(SpreadCaptureStrategy::new()));
    }
    if config.calm_persistence_enabled {
        strategies.push(Box::new(CalmPersistenceStrategy::new()));
    }
    strategies
}

/// Local receive-time pair captured when a feed message enters the live loop.
struct ReceiveTimes {
    ms: u64,
    micros: Option<u64>,
}

/// Context required to persist one live `CLOB` event into `feed_events`.
struct LiveClobLogEvent<'a> {
    receive_ms: u64,
    receive_micros: Option<u64>,
    event_ms: u64,
    event_micros: Option<u64>,
    event_type: &'a str,
    book_state: &'a crate::types::BookState,
    current_window: Option<&'a MarketWindow>,
    asset_id: Option<&'a str>,
    source_topic: Option<&'a str>,
    connection_id: &'a str,
    payload_json: Option<&'a str>,
    details_json: Option<&'a str>,
}

/// Context required to persist one feed-health event.
struct FeedHealthLogEvent<'a> {
    timestamp_ms: u64,
    timestamp_micros: Option<u64>,
    source: &'a str,
    event_type: &'a str,
    connection_id: Option<&'a str>,
    market_id: Option<&'a str>,
    details_json: Option<&'a str>,
}

const FEED_HEALTH_ROLLUP_INTERVAL_SECS: u64 = 5 * 60;
const LIVE_TRADING_CONTROL_POLL_INTERVAL_SECS: u64 = 1;
const LIVE_TRADING_POLL_INTERVAL_SECS: u64 = 15;
const CASH_CHANGE_EPSILON_USD: f64 = 0.01;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FeedHealthWindowStats {
    disconnect_count: u64,
    cumulative_downtime_ms: u64,
    max_downtime_ms: u64,
    cause_counts: std::collections::HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveFeedDisconnect {
    started_at_ms: u64,
    cause_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeedHealthRollupRow {
    source: String,
    disconnect_count: u64,
    cumulative_downtime_ms: u64,
    max_downtime_ms: u64,
    active_outage_ms: Option<u64>,
    active_cause_class: Option<String>,
    cause_counts: Vec<(String, u64)>,
}

#[derive(Default)]
struct FeedHealthTracker {
    window: std::collections::HashMap<String, FeedHealthWindowStats>,
    active: std::collections::HashMap<String, ActiveFeedDisconnect>,
}

impl FeedHealthTracker {
    /// Mark one feed connection as healthy again and accumulate completed downtime.
    fn note_connected(&mut self, source: &str, now_ms: u64) {
        if let Some(active) = self.active.remove(source) {
            let downtime_ms = now_ms.saturating_sub(active.started_at_ms);
            let stats = self.window.entry(source.to_string()).or_default();
            stats.cumulative_downtime_ms = stats.cumulative_downtime_ms.saturating_add(downtime_ms);
            stats.max_downtime_ms = stats.max_downtime_ms.max(downtime_ms);
        }
    }

    /// Record one feed disconnect if the feed is not already marked as down.
    fn note_disconnected(&mut self, source: &str, cause_class: &str, now_ms: u64) {
        if self.active.contains_key(source) {
            return;
        }

        let stats = self.window.entry(source.to_string()).or_default();
        stats.disconnect_count = stats.disconnect_count.saturating_add(1);
        *stats
            .cause_counts
            .entry(cause_class.to_string())
            .or_insert(0) += 1;
        self.active.insert(
            source.to_string(),
            ActiveFeedDisconnect {
                started_at_ms: now_ms,
                cause_class: cause_class.to_string(),
            },
        );
    }

    /// Drain the current rollup window into operator-facing rows while preserving active outages.
    fn take_rollups(&mut self, now_ms: u64) -> Vec<FeedHealthRollupRow> {
        let window = std::mem::take(&mut self.window);
        let mut sources = window.keys().cloned().collect::<Vec<_>>();
        for source in self.active.keys() {
            if !sources.iter().any(|existing| existing == source) {
                sources.push(source.clone());
            }
        }
        sources.sort();

        let mut rows = Vec::new();
        for source in sources {
            let mut stats = window.get(&source).cloned().unwrap_or_default();
            let active = self.active.get(&source);
            if stats.disconnect_count == 0 && active.is_none() {
                continue;
            }
            let mut cause_counts = stats.cause_counts.drain().collect::<Vec<_>>();
            cause_counts
                .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            rows.push(FeedHealthRollupRow {
                source: source.clone(),
                disconnect_count: stats.disconnect_count,
                cumulative_downtime_ms: stats.cumulative_downtime_ms,
                max_downtime_ms: stats.max_downtime_ms,
                active_outage_ms: active.map(|entry| now_ms.saturating_sub(entry.started_at_ms)),
                active_cause_class: active.map(|entry| entry.cause_class.clone()),
                cause_counts,
            });
        }
        rows
    }
}

impl LiveState {
    /// Creates a new `LiveState`.
    fn new() -> Self {
        Self {
            signal_state: SignalState::new(),
            current_window: None,
            window_open_prices: std::collections::HashMap::new(),
            known_windows: std::collections::HashMap::new(),
        }
    }
}

/// Capture the current local receive timestamps for one incoming live event.
fn capture_receive_times(clock: &dyn Clock) -> ReceiveTimes {
    ReceiveTimes {
        ms: clock.now(),
        micros: Some(now_us()),
    }
}

/// Run the long-lived runtime for paper or readonly venue execution.
///
/// The bot runs until `shutdown_rx` fires, at which point it performs a
/// graceful shutdown (logs final stats, closes the database).
///
/// In the CLI binary, `shutdown_rx` is wired to `tokio::signal::ctrl_c()`.
/// In tests, it is fired by the test harness after a scripted scenario.
pub async fn run_live(
    config: Config,
    db_path: &str,
    balance: f64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    config.validate()?;
    run_live_runtime(config, db_path, balance, shutdown_rx).await
}

/// Run the shared paper or readonly live runtime.
#[allow(clippy::too_many_lines)]
async fn run_live_runtime(
    config: Config,
    db_path: &str,
    balance: f64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let db = Database::new(db_path)?;
    let clock = SystemClock;
    let mut readonly_monitor = None;
    let mut live_trading_monitor = None;
    let (db, runtime_balance) =
        if config.execution_mode == crate::config::ExecutionMode::LiveReadonly {
            let (db, bootstrap) =
                crate::live_readonly::bootstrap_readonly_runtime(&config, db, &clock).await?;
            readonly_monitor = Some(bootstrap.monitor);
            (db, bootstrap.shadow_starting_balance)
        } else if config.execution_mode == crate::config::ExecutionMode::LiveTrading {
            let (db, bootstrap) =
                bootstrap_live_trading_runtime(&config, db, db_path, balance, &clock).await?;
            live_trading_monitor = Some(bootstrap.monitor);
            (db, bootstrap.live_starting_balance)
        } else {
            (db, balance)
        };
    let pending_policy = config.pending_settlement_policy_unchecked();
    db.set_run_metadata(
        "feed_event_storage_profile",
        config.feed_event_storage_profile.as_str(),
        clock.now(),
    )?;
    db.set_run_metadata(
        "replay_quality_class",
        initial_replay_quality_class(config.feed_event_storage_profile),
        clock.now(),
    )?;
    db.set_run_metadata(
        "required_feed_event_classes",
        required_feed_event_classes(),
        clock.now(),
    )?;
    info!(
        balance = runtime_balance,
        db = %db_path,
        execution_mode = config.execution_mode.as_str(),
        feed_event_storage_profile = config.feed_event_storage_profile.as_str(),
        replay_quality_class = initial_replay_quality_class(config.feed_event_storage_profile),
        pending_settlement_mode = pending_policy.mode.as_str(),
        pending_settlement_family_reserve_fraction = pending_policy.family_reserve_fraction,
        pending_settlement_global_reserve_fraction = pending_policy.global_reserve_fraction,
        pending_settlement_counts_as_open_position = pending_policy.counts_as_open_position,
        "starting live runtime"
    );
    let mut bankroll = BankrollManager::new(runtime_balance, &config, &db, &clock);
    let mut position_manager = PositionManager::new();
    let mut execution_engine = ExecutionEngine::new();
    let mut circuit_breaker = CircuitBreaker::new(
        config.circuit_breaker_losses as u32,
        config.circuit_breaker_pause_ms,
    );
    let mut trend_tracker = ScopedTrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
        config.trend_filter_per_strategy,
    );

    let mut strategies = build_strategies(&config);
    let enabled_strategies = config.enabled_strategy_names();
    if strategies.is_empty() {
        bail!(
            "no strategies enabled after config parsing; boolean env values must be true/false or 1/0"
        );
    }
    info!(
        enabled_strategies = %enabled_strategies.join(","),
        strategy_count = enabled_strategies.len(),
        "live strategy set resolved"
    );

    let mut momentum = MomentumCalculator::new(config.momentum_window_ms);

    let (feed_tx, mut feed_rx) = tokio::sync::mpsc::channel::<FeedMessage>(512);

    let binance_config = config.clone();
    let binance_tx = feed_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::feeds::binance_feed::run_binance_feed(&binance_config, binance_tx).await
        {
            error!(feed = "binance", "feed exited with error: {e}");
        }
    });

    let chainlink_config = config.clone();
    let chainlink_tx = feed_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::feeds::chainlink_feed::run_chainlink_feed(&chainlink_config, chainlink_tx).await
        {
            error!(feed = "chainlink", "feed exited with error: {e}");
        }
    });

    let clob_config = config.clone();
    let clob_tx = feed_tx.clone();
    let (clob_handle, _clob_join) =
        crate::feeds::clob_feed::run_clob_feed(&clob_config, clob_tx).await;

    let mut discovery = market_discovery::run_market_discovery(&config).await;

    let tick_logger_state = Arc::new(tokio::sync::RwLock::new(TickLoggerState::default()));
    let tick_logger_state_clone = Arc::clone(&tick_logger_state);
    let tick_interval = config.tick_interval;
    let tick_logger_db_path = db_path.to_string();
    tokio::spawn(async move {
        tick_logger::run_tick_logger(tick_logger_db_path, tick_interval, tick_logger_state_clone)
            .await;
    });

    let mut state = LiveState::new();
    let mut storage_state = FeedEventStorageState::new(config.feed_event_storage_profile);
    let mut feed_health_tracker = FeedHealthTracker::default();
    let mut rejection_tracker = StrategyRejectionTracker::new();
    let mut pending_resolutions = seed_pending_resolutions(&db, &config, &clock);

    let (activate_tx, mut activate_rx) = tokio::sync::mpsc::channel::<MarketWindow>(32);
    let mut storage_report_timer = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
    storage_report_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let _ = storage_report_timer.tick().await;
    let mut feed_health_report_timer = tokio::time::interval(std::time::Duration::from_secs(
        FEED_HEALTH_ROLLUP_INTERVAL_SECS,
    ));
    feed_health_report_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let _ = feed_health_report_timer.tick().await;
    let mut resolution_retry_timer = tokio::time::interval(std::time::Duration::from_secs(1));
    resolution_retry_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut readonly_poll_timer = if readonly_monitor.is_some() {
        let mut timer = tokio::time::interval(Duration::from_secs(
            crate::live_readonly::READONLY_POLL_INTERVAL_SECS,
        ));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = timer.tick().await;
        Some(timer)
    } else {
        None
    };
    let mut readonly_rollup_timer = if readonly_monitor.is_some() {
        let mut timer = tokio::time::interval(Duration::from_secs(
            crate::live_readonly::READONLY_ROLLUP_INTERVAL_SECS,
        ));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let _ = timer.tick().await;
        Some(timer)
    } else {
        None
    };
    let mut live_control_timer = if live_trading_monitor.is_some() {
        let mut timer =
            tokio::time::interval(Duration::from_secs(LIVE_TRADING_CONTROL_POLL_INTERVAL_SECS));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = timer.tick().await;
        Some(timer)
    } else {
        None
    };
    let mut live_poll_timer = if live_trading_monitor.is_some() {
        let mut timer = tokio::time::interval(Duration::from_secs(LIVE_TRADING_POLL_INTERVAL_SECS));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = timer.tick().await;
        Some(timer)
    } else {
        None
    };

    info!("all tasks spawned, entering main loop");

    let mut shutdown_rx = shutdown_rx;

    loop {
        tokio::select! {

            msg = feed_rx.recv() => {
                let Some(msg) = msg else {
                    warn!("all feed senders dropped, shutting down");
                    break;
                };

                match msg {
                    FeedMessage::BinanceTrade {
                        price,
                        quantity,
                        signed_quantity,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state.signal_state.update_binance_trade(
                            price,
                            quantity,
                            signed_quantity,
                            receive.ms,
                            receive.micros,
                        );
                        momentum.push(price, receive.ms);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.binance_price = Some(price);
                        }

                        let event = storage_state.prepare_binance_trade(
                            FeedEvent {
                                id: None,
                                received_at_ms: receive.ms,
                                event_at_ms: timestamp_ms,
                                received_at_us: receive.micros,
                                event_at_us: source_micros,
                                source: "binance".to_string(),
                                event_type: "aggTrade".to_string(),
                                source_topic,
                                source_symbol,
                                connection_id: Some(connection_id),
                                sequence_key,
                                market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                                asset_id: None,
                                price: Some(price),
                                trade_size: None,
                                signed_quantity: None,
                                best_bid: None,
                                best_ask: None,
                                bid_size: None,
                                ask_size: None,
                                depth_bid_notional: None,
                                depth_ask_notional: None,
                                depth_imbalance: None,
                                microprice: None,
                                payload_json,
                                details_json,
                                fidelity: ReplayFidelity::RawEvent,
                            },
                            quantity,
                            signed_quantity,
                        );
                        let _ = persist_feed_event(&db, &mut storage_state, &event);

                        if let Some(ref w) = state.current_window {
                            state.window_open_prices.entry(w.market_id.clone()).or_insert(price);
                        }

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::BinanceBookTicker {
                        best_bid,
                        best_ask,
                        bid_size,
                        ask_size,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state.signal_state.update_binance_book(
                            best_bid,
                            best_ask,
                            bid_size,
                            ask_size,
                            receive.ms,
                            sequence_key.clone(),
                        );
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Some(event) = storage_state.prepare_binance_book_ticker(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "binance".to_string(),
                            event_type: "bookTicker".to_string(),
                            source_topic,
                            source_symbol,
                            connection_id: Some(connection_id),
                            sequence_key,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: None,
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: Some(best_bid),
                            best_ask: Some(best_ask),
                            bid_size: Some(bid_size),
                            ask_size: Some(ask_size),
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        }) {
                            let _ = persist_feed_event(&db, &mut storage_state, &event);
                        }

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::BinanceDepth {
                        bid_levels,
                        ask_levels,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        let best_bid = bid_levels.first().map(|level| level.price);
                        let best_ask = ask_levels.first().map(|level| level.price);
                        let bid_size = bid_levels.first().map(|level| level.size);
                        let ask_size = ask_levels.first().map(|level| level.size);
                        let depth_event = storage_state.prepare_binance_depth(
                            FeedEvent {
                                id: None,
                                received_at_ms: receive.ms,
                                event_at_ms: timestamp_ms,
                                received_at_us: receive.micros,
                                event_at_us: source_micros,
                                source: "binance".to_string(),
                                event_type: "depth".to_string(),
                                source_topic,
                                source_symbol: source_symbol.clone(),
                                connection_id: Some(connection_id.clone()),
                                sequence_key: sequence_key.clone(),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                                asset_id: None,
                                price: None,
                                trade_size: None,
                                signed_quantity: None,
                                best_bid,
                                best_ask,
                                bid_size,
                                ask_size,
                                depth_bid_notional: None,
                                depth_ask_notional: None,
                                depth_imbalance: None,
                                microprice: None,
                                payload_json,
                                details_json,
                                fidelity: ReplayFidelity::RawEvent,
                            },
                            source_symbol.as_deref(),
                            &bid_levels,
                            &ask_levels,
                        );
                        state.signal_state.update_binance_depth(
                            bid_levels,
                            ask_levels,
                            receive.ms,
                            sequence_key.clone(),
                        );
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Some(event) = depth_event {
                            let _ = persist_feed_event(&db, &mut storage_state, &event);
                        }

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::ChainlinkPrice {
                        price,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_chainlink(price, receive.ms, receive.micros);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.chainlink_price = Some(price);
                        }

                        let event = storage_state.prepare_chainlink_price(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "chainlink".to_string(),
                            event_type: "chainlink_price".to_string(),
                            source_topic,
                            source_symbol,
                            connection_id: Some(connection_id),
                            sequence_key: None,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: Some(price),
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        });
                        let _ = persist_feed_event(&db, &mut storage_state, &event);

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );
                    }

                    FeedMessage::ClobBook {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        let _ = log_live_clob_event(
                            &db,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "book",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                        );

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::ClobPriceChange {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        let _ = log_live_clob_event(
                            &db,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "price_change",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                        );

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::ClobBestBidAsk {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        let _ = log_live_clob_event(
                            &db,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "best_bid_ask",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                        );

                        process_due_orders_and_log(
                            &mut execution_engine,
                            receive.ms,
                            receive.micros,
                            state.current_window.as_ref(),
                            &state.signal_state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );

                        let live_orders = evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut execution_engine,
                            &mut bankroll,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                            &mut rejection_tracker,
                            live_trading_monitor
                                .as_ref()
                                .is_none_or(LiveTradingMonitor::can_submit_orders),
                            receive.ms,
                        );
                        submit_live_orders_if_any(
                            live_trading_monitor.as_mut(),
                            db_path,
                            &config,
                            &mut bankroll,
                            state.current_window.as_ref(),
                            live_orders,
                            receive.ms,
                        )
                        .await;
                    }

                    FeedMessage::ClobMetaEvent {
                        event_type,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        if let Some(event) = storage_state.prepare_clob_meta(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "clob".to_string(),
                            event_type,
                            source_topic,
                            source_symbol: None,
                            connection_id: Some(connection_id),
                            sequence_key: None,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id,
                            price: None,
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        }) {
                            let _ = persist_feed_event(&db, &mut storage_state, &event);
                        }
                    }

                    FeedMessage::FeedConnected { name, connection_id } => {
                        info!(feed = %name, "feed connected");
                        feed_health_tracker.note_connected(&name, clock.now());
                        let _ = log_feed_health_event(
                            &db,
                            &FeedHealthLogEvent {
                                timestamp_ms: clock.now(),
                                timestamp_micros: Some(now_us()),
                                source: &name,
                                event_type: "connected",
                                connection_id: Some(&connection_id),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.as_str()),
                                details_json: None,
                            },
                        );
                    }

                    FeedMessage::FeedDisconnected {
                        name,
                        connection_id,
                        cause_class,
                        details_json,
                    } => {
                        warn!(feed = %name, cause_class, "feed disconnected");
                        feed_health_tracker.note_disconnected(&name, cause_class, clock.now());
                        let _ = log_feed_health_event(
                            &db,
                            &FeedHealthLogEvent {
                                timestamp_ms: clock.now(),
                                timestamp_micros: Some(now_us()),
                                source: &name,
                                event_type: "disconnected",
                                connection_id: connection_id.as_deref(),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.as_str()),
                                details_json: details_json.as_deref(),
                            },
                        );
                    }

                    FeedMessage::ChainlinkStale {
                        connection_id,
                        details_json,
                    } => {
                        warn!("chainlink price is stale");
                        state.signal_state.chainlink_price = None;
                        let _ = log_feed_health_event(
                            &db,
                            &FeedHealthLogEvent {
                                timestamp_ms: clock.now(),
                                timestamp_micros: Some(now_us()),
                                source: "chainlink",
                                event_type: "stale",
                                connection_id: connection_id.as_deref(),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.as_str()),
                                details_json: details_json.as_deref(),
                            },
                        );
                    }
                }
            }

            event = discovery.window_rx.recv() => {
                let Some(event) = event else {
                    warn!("market discovery channel closed");
                    continue;
                };

                match event {
                    MarketDiscoveryEvent::NewWindow(window) => {
                        info!(
                            market_id = %window.market_id,
                            question = %window.question,
                            "new market window discovered"
                        );

                        if let Err(e) = db.upsert_market(&window) {
                            error!("failed to upsert market: {e}");
                        }

                        state.known_windows.insert(window.market_id.clone(), window.clone());

                        let now_ms = clock.now();
                        if window.start_time <= now_ms {

                            activate_window(&db, &mut state, &window, &clob_handle);
                        } else {

                            let delay_ms = window.start_time.saturating_sub(now_ms);
                            let tx = activate_tx.clone();
                            let w = window.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                                let _ = tx.send(w).await;
                            });
                            info!(
                                market_id = %window.market_id,
                                delay_ms,
                                "window activation scheduled"
                            );
                        }
                    }

                    MarketDiscoveryEvent::WindowClosed(closed_window) => {
                        info!(market_id = %closed_window.market_id, "market window closed");

                        let closed_id = closed_window.market_id.clone();
                        if let Some(window) = state.known_windows.remove(&closed_id) {
                            if let Err(e) = db.resolve_market(&closed_id, "closed") {
                                warn!(market_id = %closed_id, "failed to mark market closed: {e}");
                            }
                            let open = state.window_open_prices.remove(&closed_id).unwrap_or_else(|| {
                                warn!(market_id = %closed_id, "no cached open price captured, recovering from persisted ticks");
                                recover_window_open_price(&db, &state, &window).unwrap_or(0.0)
                            });
                            let close = state.signal_state.binance_price.unwrap_or(open);
                            let provisional_outcome = if close >= open {
                                SignalDirection::Up
                            } else {
                                SignalDirection::Down
                            };
                            info!(
                                market_id = %closed_id,
                                provisional_outcome = %provisional_outcome,
                                provisional_open = open,
                                provisional_close = close,
                                "window closed; awaiting authoritative resolution before settlement"
                            );

                            match db.get_open_trades_for_market(&closed_id) {
                                Ok(trades) if !trades.is_empty() => {
                                    for trade in &trades {
                                        bankroll.transition_trade_to_pending_settlement(
                                            trade.entry_price * trade.size,
                                            &trade.strategy,
                                        );
                                    }
                                    let first_attempt_at_ms = window
                                        .end_time
                                        .saturating_add(config.resolution_initial_delay_ms)
                                        .max(clock.now());
                                    schedule_pending_resolution(
                                        &mut pending_resolutions,
                                        &window,
                                        first_attempt_at_ms,
                                        false,
                                    );
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    warn!(
                                        market_id = %closed_id,
                                        "failed to load open trades for pending-settlement reclassification: {e}"
                                    );
                                }
                            }

                            if state.current_window.as_ref().is_some_and(|w| w.market_id == closed_id) {
                                state.current_window = None;
                                state.signal_state.book_state = crate::types::BookState::default();
                            }

                            flush_rejection_summaries_for_market(
                                &db,
                                &mut rejection_tracker,
                                &closed_id,
                                clock.now(),
                            );
                            log_execution_rollup_for_market(&db, &closed_id);
                        }
                    }
                }
            }

            window = activate_rx.recv() => {
                if let Some(window) = window {
                    activate_window(&db, &mut state, &window, &clob_handle);
                }
            }

            _ = storage_report_timer.tick() => {
                log_rejection_rollups(&rejection_tracker.snapshot_all(clock.now()));
                if let Ok(footprint) = db.storage_footprint() {
                    let rows = storage_state.take_row_counts();
                    let row_summary = rows
                        .iter()
                        .map(|(key, count)| format!("{key}={count}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    info!(
                        db_bytes = footprint.db_bytes,
                        wal_bytes = footprint.wal_bytes,
                        feed_events = footprint.feed_event_count,
                        rows = row_summary,
                        replay_quality_class = footprint_replay_quality_class(&footprint.grouped_feed_events),
                        "live storage footprint"
                    );
                    let now = clock.now();
                    if let Err(error) = db.set_run_metadata(
                        "replay_quality_class",
                        footprint_replay_quality_class(&footprint.grouped_feed_events),
                        now,
                    ) {
                        warn!(%error, "failed to persist replay quality metadata");
                    }
                    if let Err(error) = db.set_run_metadata("feed_event_classes", &row_summary, now) {
                        warn!(%error, "failed to persist feed class metadata");
                    }
                }
            }

            _ = feed_health_report_timer.tick() => {
                log_feed_health_rollups(&feed_health_tracker.take_rollups(clock.now()));
            }

            _ = resolution_retry_timer.tick() => {
                let ready_pending = take_ready_pending_resolutions(&mut pending_resolutions, clock.now());
                for mut pending in ready_pending {
                    match db.get_open_trades_for_market(&pending.market_id) {
                        Ok(trades) if trades.is_empty() => {
                            tracing::debug!(
                                market_id = %pending.market_id,
                                "dropping pending authoritative settlement for market with no open trades"
                            );
                            continue;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!(
                                market_id = %pending.market_id,
                                "failed to inspect open trades before authoritative settlement retry: {e}"
                            );
                            pending.next_attempt_at_ms = clock.now().saturating_add(config.resolution_poll_delay_ms);
                            pending_resolutions.insert(pending.market_id.clone(), pending);
                            continue;
                        }
                    }

                    let slug = pending.window.slug.clone();
                    let gamma_api_url = config.gamma_api_url.clone();
                    match crate::market_discovery::fetch_resolution_once(&gamma_api_url, &slug).await {
                        Ok(Some(outcome)) => {
                            handle_authoritative_resolution(
                                &pending.window,
                                outcome,
                                pending.seeded_from_startup,
                                &db,
                                &mut position_manager,
                                &mut bankroll,
                                &mut trend_tracker,
                                &mut circuit_breaker,
                                &config,
                                &clock,
                            );
                        }
                        Ok(None) => {
                            tracing::debug!(
                                market_id = %pending.market_id,
                                slug = %pending.window.slug,
                                "authoritative settlement still unresolved, will retry"
                            );
                            pending.next_attempt_at_ms =
                                clock.now().saturating_add(config.resolution_poll_delay_ms);
                            pending_resolutions.insert(pending.market_id.clone(), pending);
                        }
                        Err(e) => {
                            warn!(
                                market_id = %pending.market_id,
                                slug = %pending.window.slug,
                                "authoritative settlement fetch failed: {e}"
                            );
                            pending.next_attempt_at_ms =
                                clock.now().saturating_add(config.resolution_poll_delay_ms);
                            pending_resolutions.insert(pending.market_id.clone(), pending);
                        }
                    }
                }
            }

            () = async {
                if let Some(timer) = readonly_poll_timer.as_mut() {
                    timer.tick().await;
                }
            }, if readonly_monitor.is_some() => {
                if let Some(monitor) = readonly_monitor.as_mut() {
                    match monitor.fetch_account_state().await {
                        Ok(account) => monitor.apply_account_state(&db, &config, account)?,
                        Err(error) => {
                            monitor.record_account_refresh_failure(
                                &db,
                                &config,
                                &clock,
                                &error.to_string(),
                            )?;
                        }
                    }
                }
            }

            () = async {
                if let Some(timer) = readonly_rollup_timer.as_mut() {
                    timer.tick().await;
                }
            }, if readonly_monitor.is_some() => {
                if let Some(monitor) = readonly_monitor.as_ref() {
                    monitor.log_shadow_rollup(&bankroll.get_stats());
                }
            }

            () = async {
                if let Some(timer) = live_control_timer.as_mut() {
                    timer.tick().await;
                }
            }, if live_trading_monitor.is_some() => {
                if let Some(monitor) = live_trading_monitor.as_mut() {
                    if let Err(error) = monitor.apply_pending_controls(db_path, &config, &clock).await {
                        error!("failed to apply live-control command: {error}");
                    }
                }
            }

            () = async {
                if let Some(timer) = live_poll_timer.as_mut() {
                    timer.tick().await;
                }
            }, if live_trading_monitor.is_some() => {
                if let Some(monitor) = live_trading_monitor.as_mut() {
                    if let Err(error) = monitor.refresh_remote_state(db_path, &config, &clock).await {
                        error!("failed to refresh live-trading remote state: {error}");
                    }
                }
            }

            _ = &mut shutdown_rx => {
                info!("shutdown signal received, shutting down");
                break;
            }
        }
    }

    flush_all_rejection_summaries(&db, &mut rejection_tracker, clock.now());

    let final_stats = bankroll.get_stats();
    info!(
        starting_balance = final_stats.starting_balance,
        final_balance = final_stats.current_balance,
        total_pnl = final_stats.total_pnl,
        trades = final_stats.total_trades,
        wins = final_stats.wins,
        losses = final_stats.losses,
        win_rate = format!("{:.1}%", final_stats.win_rate * 100.0),
        max_drawdown = format!("{:.1}%", final_stats.max_drawdown_pct * 100.0),
        hwm = final_stats.high_water_mark,
        "final bankroll stats"
    );

    if let Some(monitor) = readonly_monitor.as_mut() {
        monitor.finish_stopped(&db, &clock)?;
    }
    if let Some(monitor) = live_trading_monitor.as_mut() {
        monitor.finish_stopped(&db, &clock)?;
    }

    db.close();
    info!("database closed, goodbye");

    Ok(())
}

impl LiveTradingMonitor {
    /// Return whether new venue submissions may be attempted right now.
    fn can_submit_orders(&self) -> bool {
        self.state == "armed" && self.blocked_reason.is_none()
    }

    /// Apply every queued operator command in durable request order.
    async fn apply_pending_controls(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
    ) -> anyhow::Result<()> {
        let commands = {
            let db = Database::new(db_path)?;
            let commands = db.pending_live_control_commands(self.session_id)?;
            db.close();
            commands
        };
        for command in commands {
            let command_id = command
                .id
                .context("pending live-control command has no id")?;
            let action = match LiveControlAction::from_str(&command.action) {
                Ok(action) => action,
                Err(error) => {
                    let details = json!({ "error": error.to_string() }).to_string();
                    let db = Database::new(db_path)?;
                    db.update_live_control_command_status(
                        command_id,
                        clock.now(),
                        "rejected",
                        Some(&details),
                    )?;
                    db.close();
                    continue;
                }
            };
            let applied_at_ms = clock.now();
            let result = self
                .apply_control_action(
                    db_path,
                    config,
                    clock,
                    action,
                    &command.actor,
                    &command.reason,
                )
                .await;
            let (status, details) = match result {
                Ok(details) => ("applied", details),
                Err(error) => {
                    warn!(
                        command_id,
                        action = %action.as_str(),
                        "live-control command rejected: {error}"
                    );
                    (
                        "rejected",
                        json!({
                            "action": action.as_str(),
                            "error": error.to_string(),
                        }),
                    )
                }
            };
            let db = Database::new(db_path)?;
            db.update_live_control_command_status(
                command_id,
                applied_at_ms,
                status,
                Some(&details.to_string()),
            )?;
            db.close();
        }
        Ok(())
    }

    /// Refresh preflight, account, and activity state from the sidecar.
    async fn refresh_remote_state(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
    ) -> anyhow::Result<Vec<String>> {
        let preflight = self.sidecar.preflight(config).await?;
        let account = self.sidecar.account_state().await?;
        let activity = self.sidecar.activity().await?;
        let db = Database::new(db_path)?;
        db.log_live_account_snapshot(&live_account_snapshot(self.session_id, &account))?;
        self.persist_activity_recovery(&db, &activity)?;
        let issues = live_gate_issues(&preflight, &account, &activity, config);
        self.preflight = Some(preflight);
        self.account = Some(account);
        self.activity = Some(activity);
        self.blocked_reason = issues.first().cloned();
        if self.state == "armed" && self.blocked_reason.is_some() {
            let details = json!({ "issues": issues }).to_string();
            self.set_state(
                &db,
                "unknown_order",
                "system",
                "remote state degraded while armed",
                clock.now(),
                Some(&details),
            )?;
        } else {
            self.update_session_metadata(&db)?;
        }
        db.close();
        Ok(issues)
    }

    /// Submit a batch of live orders through the authenticated sidecar boundary.
    async fn submit_orders(
        &mut self,
        db_path: &str,
        config: &Config,
        bankroll: &mut BankrollManager,
        window: &MarketWindow,
        orders: &[QueuedOrderIntent],
        now_ms: u64,
    ) -> anyhow::Result<()> {
        if orders.is_empty() {
            return Ok(());
        }
        if !self.can_submit_orders() {
            bail!(
                "live order submission blocked by state={} reason={}",
                self.state,
                self.blocked_reason.as_deref().unwrap_or("none")
            );
        }
        let mut successful = 0_u64;
        for order in orders {
            if self
                .submit_one_order(db_path, config, bankroll, window, order, now_ms)
                .await?
            {
                successful += 1;
            }
            if !self.can_submit_orders() {
                break;
            }
        }
        if orders.len() == 2
            && orders
                .iter()
                .any(|order| order.strategy.as_str() == "spread-capture")
            && successful == 1
        {
            let db = Database::new(db_path)?;
            let details = json!({
                "market_id": window.market_id,
                "successful_legs": successful,
                "submitted_legs": orders.len(),
            })
            .to_string();
            self.set_state(
                &db,
                "unknown_order",
                "system",
                "spread residual exposure detected",
                now_ms,
                Some(&details),
            )?;
            db.log_live_reconciliation_event(&LiveReconciliationEvent {
                id: None,
                session_id: self.session_id,
                timestamp_ms: now_ms,
                severity: "critical".to_string(),
                event_type: "spread_residual_exposure".to_string(),
                local_value: Some(successful as f64),
                remote_value: Some(orders.len() as f64),
                details_json: Some(json!({ "market_id": window.market_id }).to_string()),
            })?;
            db.close();
        }
        Ok(())
    }

    /// Finish the current live-trading session after normal process shutdown.
    fn finish_stopped(&mut self, db: &Database, clock: &dyn Clock) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }
        let now = clock.now();
        let status = if self.state == "armed" {
            "disarmed"
        } else {
            self.state.as_str()
        };
        db.finish_live_session(
            self.session_id,
            now,
            "live_stopped",
            Some(
                &json!({
                    "previous_state": status,
                    "reason": "process_shutdown",
                })
                .to_string(),
            ),
        )?;
        self.finished = true;
        Ok(())
    }

    /// Apply one parsed operator action.
    async fn apply_control_action(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
        action: LiveControlAction,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        match action {
            LiveControlAction::Arm => self.arm(db_path, config, clock, actor, reason).await,
            LiveControlAction::Disarm => {
                let db = Database::new(db_path)?;
                self.set_state(&db, "disarmed", actor, reason, clock.now(), None)?;
                db.close();
                Ok(json!({ "state": self.state }))
            }
            LiveControlAction::StopAfterFlat => {
                let db = Database::new(db_path)?;
                self.set_state(&db, "stop_after_flat", actor, reason, clock.now(), None)?;
                db.close();
                Ok(json!({ "state": self.state }))
            }
            LiveControlAction::KillSwitch => self.kill_switch(db_path, clock, actor, reason).await,
            LiveControlAction::CancelAll => self.cancel_all(db_path, clock, actor, reason).await,
            LiveControlAction::RedeemAll => self.redeem_all(db_path, clock, actor, reason).await,
        }
    }

    /// Arm live trading only after every safety gate is healthy.
    async fn arm(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let issues = self.refresh_remote_state(db_path, config, clock).await?;
        if !issues.is_empty() {
            bail!("live arming blocked: {}", issues.join("; "));
        }
        let db = Database::new(db_path)?;
        self.set_state(&db, "armed", actor, reason, clock.now(), None)?;
        db.close();
        Ok(json!({ "state": self.state }))
    }

    /// Halt trading, attempt cancel-all, and keep the session blocked.
    async fn kill_switch(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let now = clock.now();
        let cancel_result = self.sidecar.cancel_all().await;
        let details = match cancel_result {
            Ok(response) => json!({
                "cancel_all": response,
            }),
            Err(error) => json!({
                "cancel_all_error": error.to_string(),
            }),
        };
        let db = Database::new(db_path)?;
        let details_json = details.to_string();
        self.set_state(&db, "halted", actor, reason, now, Some(&details_json))?;
        db.log_live_reconciliation_event(&LiveReconciliationEvent {
            id: None,
            session_id: self.session_id,
            timestamp_ms: now,
            severity: "critical".to_string(),
            event_type: "kill_switch_activated".to_string(),
            local_value: None,
            remote_value: None,
            details_json: Some(details.to_string()),
        })?;
        db.close();
        Ok(json!({ "state": self.state, "details": details }))
    }

    /// Cancel every open venue order without changing the arming state.
    async fn cancel_all(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let response = self.sidecar.cancel_all().await?;
        let db = Database::new(db_path)?;
        db.log_control_audit(&crate::types::ControlAuditEntry {
            id: None,
            timestamp_ms: clock.now(),
            actor: actor.to_string(),
            action: "live_cancel_all_submitted".to_string(),
            target: Some(self.session_id.to_string()),
            details_json: Some(
                json!({
                    "reason": reason,
                    "response": response,
                })
                .to_string(),
            ),
        })?;
        db.close();
        Ok(json!({ "cancel_all": response }))
    }

    /// Trigger redemption for all redeemable positions and persist the command result.
    async fn redeem_all(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let now = clock.now();
        let response = self.sidecar.redeem_all().await?;
        let db = Database::new(db_path)?;
        if response.submitted > 0 {
            db.log_live_redemption(&LiveRedemption {
                id: None,
                session_id: self.session_id,
                market_id: "all".to_string(),
                detected_redeemable_at_ms: now,
                submitted_at_ms: Some(now),
                confirmed_at_ms: None,
                cash_credit_observed_at_ms: None,
                status: "submitted".to_string(),
                redeemable_value: 0.0,
                tx_hash: None,
                details_json: response.details_json.clone(),
            })?;
        }
        db.log_control_audit(&crate::types::ControlAuditEntry {
            id: None,
            timestamp_ms: now,
            actor: actor.to_string(),
            action: "live_redeem_all_submitted".to_string(),
            target: Some(self.session_id.to_string()),
            details_json: Some(
                json!({
                    "reason": reason,
                    "response": response,
                })
                .to_string(),
            ),
        })?;
        db.close();
        Ok(json!({ "redeem_all": response }))
    }

    /// Persist one submitted live venue order and its outcome.
    async fn submit_one_order(
        &mut self,
        db_path: &str,
        config: &Config,
        bankroll: &mut BankrollManager,
        window: &MarketWindow,
        order: &QueuedOrderIntent,
        now_ms: u64,
    ) -> anyhow::Result<bool> {
        let notional = order.requested_price * order.requested_size;
        let (intent_id, reject_reason) =
            self.persist_live_order_intent(db_path, config, window, order, now_ms, notional)?;
        if let Some(reason) = reject_reason {
            bankroll.release_reserved_for_strategy(order.reserved_cost, &order.strategy);
            info!(
                intent_id,
                signal_id = order.signal_id,
                strategy = %order.strategy,
                reason,
                "live order rejected before venue submission"
            );
            return Ok(false);
        }
        let request = self.build_live_order_request(intent_id, order, notional);
        match self.sidecar.submit_order_intent(&request).await {
            Ok(response) => self
                .handle_live_order_response(db_path, bankroll, order, intent_id, now_ms, &response),
            Err(error) => {
                self.handle_live_order_error(db_path, order, &request, intent_id, now_ms, error)
            }
        }
    }

    /// Persist one live order intent before any venue submission.
    fn persist_live_order_intent(
        &self,
        db_path: &str,
        config: &Config,
        window: &MarketWindow,
        order: &QueuedOrderIntent,
        now_ms: u64,
        notional: f64,
    ) -> anyhow::Result<(i64, Option<&'static str>)> {
        let (intent_status, reject_reason) =
            live_order_pre_submit_rejection(order, notional, config);
        let db = Database::new(db_path)?;
        let intent_id = db.log_live_order_intent(&LiveOrderIntent {
            id: None,
            session_id: self.session_id,
            signal_id: Some(order.signal_id),
            market_id: order.market_id.clone(),
            strategy: order.strategy.clone(),
            side: order.side.to_string(),
            order_type: "FOK".to_string(),
            status: intent_status.to_string(),
            created_at_ms: now_ms,
            requested_price: Some(order.requested_price),
            requested_size: Some(order.requested_size),
            limit_price: Some(order.limit_price),
            fee_schedule_json: window.fee_schedule_json.clone(),
            token_fee_rates_json: window.token_fee_rates_json.clone(),
            execution_group_id: order.execution_group_id.clone(),
            details_json: Some(
                json!({
                    "signal_timestamp": order.signal_timestamp,
                    "arrival_ts": order.arrival_ts,
                    "execution_fidelity": order.execution_fidelity.to_string(),
                    "amount_usd": notional,
                    "reject_reason": reject_reason,
                })
                .to_string(),
            ),
        })?;
        db.close();
        Ok((intent_id, reject_reason))
    }

    /// Build the sidecar order request for one persisted live intent.
    fn build_live_order_request(
        &self,
        intent_id: i64,
        order: &QueuedOrderIntent,
        notional: f64,
    ) -> LiveOrderIntentRequest {
        LiveOrderIntentRequest {
            session_id: self.session_id,
            intent_id,
            market_id: order.market_id.clone(),
            token_id: order.token_id.clone(),
            side: "BUY".to_string(),
            order_type: "FOK".to_string(),
            limit_price: order.limit_price,
            size: order.requested_size,
            amount_usd: Some(notional),
            client_order_id: format!("buba-live-{}-{intent_id}", self.session_id),
            details_json: Some(
                json!({
                    "strategy": order.strategy,
                    "signal_id": order.signal_id,
                    "execution_group_id": order.execution_group_id,
                })
                .to_string(),
            ),
        }
    }

    /// Persist one successful sidecar response and return whether it filled.
    fn handle_live_order_response(
        &mut self,
        db_path: &str,
        bankroll: &mut BankrollManager,
        order: &QueuedOrderIntent,
        intent_id: i64,
        now_ms: u64,
        response: &LiveOrderIntentResponse,
    ) -> anyhow::Result<bool> {
        let db = Database::new(db_path)?;
        let live_order_id = self.persist_order_response(&db, intent_id, order, now_ms, response)?;
        if !response.ok {
            bankroll.release_reserved_for_strategy(order.reserved_cost, &order.strategy);
            if live_order_response_is_blocking(response) {
                self.set_state(
                    &db,
                    "unknown_order",
                    "system",
                    "blocking venue order response",
                    now_ms,
                    response.details_json.as_deref(),
                )?;
            }
            db.close();
            return Ok(false);
        }
        self.persist_response_fill(&db, intent_id, live_order_id, order, now_ms, response)?;
        db.close();
        Ok(true)
    }

    /// Persist one fill inferred from a successful sidecar order response.
    fn persist_response_fill(
        &self,
        db: &Database,
        intent_id: i64,
        live_order_id: i64,
        order: &QueuedOrderIntent,
        now_ms: u64,
        response: &LiveOrderIntentResponse,
    ) -> anyhow::Result<()> {
        if let Some(accepted_size) = response.accepted_size
            && accepted_size > 0.0
        {
            db.log_live_fill(&LiveFill {
                id: None,
                session_id: self.session_id,
                intent_id: Some(intent_id),
                live_order_id: Some(live_order_id),
                venue_trade_id: None,
                filled_at_ms: now_ms,
                price: order.requested_price,
                size: accepted_size,
                fee_amount: None,
                fee_rate: None,
                liquidity_side: Some("taker".to_string()),
                tx_hash: None,
                status: "venue_response_pending_reconciliation".to_string(),
                details_json: response.details_json.clone(),
            })?;
        }
        Ok(())
    }

    /// Persist an unknown sidecar submission outcome and block further trading.
    fn handle_live_order_error(
        &mut self,
        db_path: &str,
        order: &QueuedOrderIntent,
        request: &LiveOrderIntentRequest,
        intent_id: i64,
        now_ms: u64,
        error: anyhow::Error,
    ) -> anyhow::Result<bool> {
        let db = Database::new(db_path)?;
        let error_message = error.to_string();
        db.log_live_order(&LiveOrder {
            id: None,
            session_id: self.session_id,
            intent_id,
            venue_order_id: None,
            client_order_id: Some(request.client_order_id.clone()),
            market_id: order.market_id.clone(),
            token_id: Some(order.token_id.clone()),
            side: "BUY".to_string(),
            order_type: "FOK".to_string(),
            status: "unknown_submission".to_string(),
            status_reason: Some(error_message.clone()),
            created_at_ms: now_ms,
            acknowledged_at_ms: None,
            updated_at_ms: now_ms,
            requested_price: Some(order.requested_price),
            limit_price: Some(order.limit_price),
            requested_size: Some(order.requested_size),
            accepted_size: None,
            details_json: Some(json!({ "error": error_message }).to_string()),
        })?;
        let details = json!({ "error": error.to_string() }).to_string();
        self.set_state(
            &db,
            "unknown_order",
            "system",
            "sidecar order submission outcome unknown",
            now_ms,
            Some(&details),
        )?;
        db.close();
        Err(error)
    }

    /// Persist one sidecar order response as a live order row.
    fn persist_order_response(
        &self,
        db: &Database,
        intent_id: i64,
        order: &QueuedOrderIntent,
        now_ms: u64,
        response: &LiveOrderIntentResponse,
    ) -> anyhow::Result<i64> {
        db.log_live_order(&LiveOrder {
            id: None,
            session_id: self.session_id,
            intent_id,
            venue_order_id: response.venue_order_id.clone(),
            client_order_id: Some(response.client_order_id.clone()),
            market_id: order.market_id.clone(),
            token_id: Some(order.token_id.clone()),
            side: "BUY".to_string(),
            order_type: "FOK".to_string(),
            status: response.status.clone(),
            status_reason: response.status_reason.clone(),
            created_at_ms: now_ms,
            acknowledged_at_ms: Some(now_ms),
            updated_at_ms: now_ms,
            requested_price: Some(order.requested_price),
            limit_price: Some(order.limit_price),
            requested_size: Some(order.requested_size),
            accepted_size: response.accepted_size,
            details_json: response.details_json.clone(),
        })
    }

    /// Persist activity-recovered trades and user-stream health events.
    fn persist_activity_recovery(
        &self,
        db: &Database,
        activity: &LiveActivityResponse,
    ) -> anyhow::Result<()> {
        if activity.user_stream_status == LiveCheckStatus::Failed {
            db.log_live_reconciliation_event(&LiveReconciliationEvent {
                id: None,
                session_id: self.session_id,
                timestamp_ms: activity.timestamp_ms,
                severity: "critical".to_string(),
                event_type: "user_stream_unhealthy".to_string(),
                local_value: None,
                remote_value: None,
                details_json: activity.details_json.clone(),
            })?;
        }
        for trade in &activity.clob_trades {
            let Some(trade_id) = trade.trade_id.as_deref() else {
                continue;
            };
            if db.live_fill_exists(self.session_id, trade_id)? {
                continue;
            }
            let (Some(price), Some(size)) = (trade.price, trade.size) else {
                continue;
            };
            db.log_live_fill(&LiveFill {
                id: None,
                session_id: self.session_id,
                intent_id: None,
                live_order_id: None,
                venue_trade_id: Some(trade_id.to_string()),
                filled_at_ms: trade.timestamp_ms,
                price,
                size,
                fee_amount: None,
                fee_rate: None,
                liquidity_side: None,
                tx_hash: None,
                status: "confirmed_from_activity".to_string(),
                details_json: trade.details_json.clone(),
            })?;
        }
        Ok(())
    }

    /// Persist a new live-control state and refresh the session metadata.
    fn set_state(
        &mut self,
        db: &Database,
        state: &str,
        actor: &str,
        reason: &str,
        now_ms: u64,
        details_json: Option<&str>,
    ) -> anyhow::Result<()> {
        record_live_control_state(
            db,
            self.session_id,
            state,
            actor,
            reason,
            now_ms,
            details_json,
        )?;
        self.state = state.to_string();
        self.blocked_reason =
            (state == "unknown_order" || state == "halted").then(|| reason.to_string());
        self.update_session_metadata(db)?;
        Ok(())
    }

    /// Persist the current live-trading presentation state to the session row.
    fn update_session_metadata(&self, db: &Database) -> anyhow::Result<()> {
        let wallet = self
            .account
            .as_ref()
            .and_then(|account| account.wallet_address.as_deref())
            .or_else(|| {
                self.preflight
                    .as_ref()
                    .and_then(|preflight| preflight.wallet_address.as_deref())
            });
        let proxy = self
            .account
            .as_ref()
            .and_then(|account| account.proxy_wallet.as_deref())
            .or_else(|| {
                self.preflight
                    .as_ref()
                    .and_then(|preflight| preflight.proxy_wallet.as_deref())
            });
        db.update_live_session_metadata(
            self.session_id,
            &self.state,
            wallet,
            proxy,
            Some(&live_session_details_json(self)),
        )
    }
}

/// Submit live orders from one strategy cycle when a live monitor is active.
async fn submit_live_orders_if_any(
    monitor: Option<&mut LiveTradingMonitor>,
    db_path: &str,
    config: &Config,
    bankroll: &mut BankrollManager,
    window: Option<&MarketWindow>,
    orders: Vec<QueuedOrderIntent>,
    now_ms: u64,
) {
    if orders.is_empty() {
        return;
    }
    let Some(monitor) = monitor else {
        return;
    };
    let Some(window) = window else {
        error!("live orders generated without an active window");
        return;
    };
    if let Err(error) = monitor
        .submit_orders(db_path, config, bankroll, window, &orders, now_ms)
        .await
    {
        error!("failed to submit live orders: {error}");
    }
}

/// Bootstrap a disarmed live-trading runtime session.
async fn bootstrap_live_trading_runtime(
    config: &Config,
    db: Database,
    db_path: &str,
    fallback_balance: f64,
    clock: &dyn Clock,
) -> anyhow::Result<(Database, LiveTradingRuntimeBootstrap)> {
    let started_at_ms = clock.now();
    let enabled_strategies = serde_json::to_string(&config.enabled_strategy_names())
        .context("serializing strategies")?;
    let session_id = db.insert_live_session(&LiveSession {
        id: None,
        started_at_ms,
        ended_at_ms: None,
        status: "disarmed".to_string(),
        execution_mode: config.execution_mode.as_str().to_string(),
        wallet_address: None,
        proxy_wallet: None,
        enabled_strategies_json: enabled_strategies,
        config_fingerprint: live_trading_config_fingerprint(config),
        cash_cap_usd: config.live_session_cash_cap_usd,
        details_json: Some(json!({ "state": "disarmed" }).to_string()),
    })?;
    record_live_control_state(
        &db,
        session_id,
        "disarmed",
        "system",
        "live_trading starts disarmed",
        started_at_ms,
        None,
    )?;
    let mut monitor = LiveTradingMonitor {
        sidecar: LiveSidecarClient::new(&config.live_sidecar_url),
        session_id,
        state: "disarmed".to_string(),
        preflight: None,
        account: None,
        activity: None,
        blocked_reason: None,
        finished: false,
    };
    db.close();
    if let Err(error) = monitor.refresh_remote_state(db_path, config, clock).await {
        warn!(
            session_id,
            "live_trading started with degraded sidecar state: {error}"
        );
        monitor.blocked_reason = Some(error.to_string());
        let db = Database::new(db_path)?;
        monitor.update_session_metadata(&db)?;
        db.close();
    }
    let live_starting_balance = monitor
        .account
        .as_ref()
        .map_or(fallback_balance, |account| {
            account
                .cash_available
                .max(0.0)
                .min(config.live_session_cash_cap_usd)
        });
    info!(
        session_id,
        live_starting_balance,
        state = %monitor.state,
        "live_trading runtime bootstrapped disarmed"
    );
    let db = Database::new(db_path)?;
    Ok((
        db,
        LiveTradingRuntimeBootstrap {
            live_starting_balance,
            monitor,
        },
    ))
}

/// Convert one sidecar account response into the persistent account snapshot.
fn live_account_snapshot(session_id: i64, account: &LiveAccountState) -> LiveAccountSnapshot {
    LiveAccountSnapshot {
        id: None,
        session_id,
        timestamp_ms: account.timestamp_ms,
        cash_available: account.cash_available,
        cash_reserved_for_orders: account.cash_reserved_for_orders,
        inventory_mark_value: account.inventory_mark_value,
        redeemable_value: account.redeemable_value,
        pending_redeem_value: account.pending_redeem_value,
        total_equity: account.total_equity,
        allowance_available: account.allowance_available,
        details_json: account.details_json.clone(),
    }
}

/// Return all current arming blockers from venue and local safety state.
fn live_gate_issues(
    preflight: &LivePreflightResponse,
    account: &LiveAccountState,
    activity: &LiveActivityResponse,
    config: &Config,
) -> Vec<String> {
    let mut issues = Vec::new();
    if !preflight.ok {
        issues.push("preflight failed".to_string());
    }
    if preflight.geoblock_status == LiveCheckStatus::Failed {
        issues.push("geoblock check failed".to_string());
    }
    if preflight.auth_status == LiveCheckStatus::Failed {
        issues.push("auth bootstrap failed".to_string());
    }
    if preflight.clock_status == LiveCheckStatus::Failed {
        issues.push("clock drift check failed".to_string());
    }
    if preflight.user_stream_status == LiveCheckStatus::Failed
        || activity.user_stream_status == LiveCheckStatus::Failed
    {
        issues.push("user stream is unhealthy".to_string());
    }
    if account.cash_available + CASH_CHANGE_EPSILON_USD < config.live_min_required_cash_usd {
        issues.push("cash below live minimum".to_string());
    }
    if account.allowance_available.is_none() {
        issues.push("allowance unavailable".to_string());
    }
    if config.feed_event_storage_profile == FeedEventStorageProfile::Compact {
        issues.push("feed capture is not replay-grade".to_string());
    }
    issues.extend(preflight.errors.clone());
    issues.sort();
    issues.dedup();
    issues
}

/// Return one deterministic live-trading config fingerprint.
fn live_trading_config_fingerprint(config: &Config) -> String {
    json!({
        "execution_mode": config.execution_mode.as_str(),
        "live_sidecar_url": config.live_sidecar_url,
        "enabled_strategies": config.enabled_strategy_names(),
        "cash_cap_usd": config.live_session_cash_cap_usd,
        "max_single_order_usd": config.live_max_single_order_usd,
        "max_open_notional_usd": config.live_max_open_notional_usd,
        "max_daily_loss_usd": config.live_max_daily_loss_usd,
        "max_session_drawdown_usd": config.live_max_session_drawdown_usd,
        "feed_event_storage_profile": config.feed_event_storage_profile.as_str(),
    })
    .to_string()
}

/// Build compact live-trading details for the session row.
fn live_session_details_json(monitor: &LiveTradingMonitor) -> String {
    json!({
        "state": monitor.state,
        "blocked_reason": monitor.blocked_reason,
        "preflight": monitor.preflight.as_ref().map(live_preflight_summary_json),
        "account": monitor.account.as_ref().map(live_account_summary_json),
        "activity": monitor.activity.as_ref().map(live_activity_summary_json),
    })
    .to_string()
}

/// Build a compact preflight summary for live session details.
fn live_preflight_summary_json(preflight: &LivePreflightResponse) -> serde_json::Value {
    json!({
        "ok": preflight.ok,
        "mode": preflight.mode,
        "geoblock_status": live_check_status_label(preflight.geoblock_status),
        "auth_status": live_check_status_label(preflight.auth_status),
        "clock_status": live_check_status_label(preflight.clock_status),
        "allowance_status": live_check_status_label(preflight.allowance_status),
        "user_stream_status": live_check_status_label(preflight.user_stream_status),
        "available_cash_usd": preflight.available_cash_usd,
        "legal_order_min_usd": preflight.legal_order_min_usd,
        "errors": preflight.errors,
    })
}

/// Build a compact account summary for live session details.
fn live_account_summary_json(account: &LiveAccountState) -> serde_json::Value {
    json!({
        "timestamp_ms": account.timestamp_ms,
        "cash_available": account.cash_available,
        "cash_reserved_for_orders": account.cash_reserved_for_orders,
        "inventory_mark_value": account.inventory_mark_value,
        "redeemable_value": account.redeemable_value,
        "pending_redeem_value": account.pending_redeem_value,
        "total_equity": account.total_equity,
        "allowance_available": account.allowance_available,
    })
}

/// Build a compact activity summary for live session details.
fn live_activity_summary_json(activity: &LiveActivityResponse) -> serde_json::Value {
    json!({
        "timestamp_ms": activity.timestamp_ms,
        "user_stream_status": live_check_status_label(activity.user_stream_status),
        "user_stream_event_count": activity.user_stream_events.len(),
        "clob_trade_count": activity.clob_trades.len(),
    })
}

/// Return the serialized label for a live check status.
fn live_check_status_label(status: LiveCheckStatus) -> &'static str {
    match status {
        LiveCheckStatus::Ok => "ok",
        LiveCheckStatus::Failed => "failed",
    }
}

/// Return the pre-submit status and rejection reason for one live order.
fn live_order_pre_submit_rejection(
    order: &QueuedOrderIntent,
    notional: f64,
    config: &Config,
) -> (&'static str, Option<&'static str>) {
    if order.requested_price <= 0.0 || order.requested_size <= 0.0 || order.limit_price <= 0.0 {
        return ("rejected_before_submit", Some("invalid_price_or_size"));
    }
    if notional + CASH_CHANGE_EPSILON_USD < config.min_bet_usd {
        return ("rejected_before_submit", Some("below_min_bet"));
    }
    if notional > config.live_max_single_order_usd + CASH_CHANGE_EPSILON_USD {
        return (
            "rejected_before_submit",
            Some("above_live_max_single_order"),
        );
    }
    ("submitted", None)
}

/// Return whether one order response blocks all future live submissions.
fn live_order_response_is_blocking(response: &LiveOrderIntentResponse) -> bool {
    matches!(
        response.status.as_str(),
        "unknown_submission" | "venue_restart" | "timeout" | "pending_unknown"
    )
}

/// Activate a market window: set it as current, resubscribe the `CLOB` feed to
/// the window's tokens, reset the book state, and capture the open price.
fn activate_window(
    db: &Database,
    state: &mut LiveState,
    window: &MarketWindow,
    clob_handle: &crate::feeds::clob_feed::ClobFeedHandle,
) {
    info!(
        market_id = %window.market_id,
        "window activated (now trading)"
    );

    let up_token = window.up_token_id.clone();
    let down_token = window.down_token_id.clone();
    let handle = clob_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = handle.resubscribe(up_token, down_token).await {
            tracing::error!("failed to resubscribe CLOB feed: {e}");
        }
    });

    state.signal_state.book_state = crate::types::BookState::default();

    if let Some(open_price) = recover_window_open_price(db, state, window) {
        state
            .window_open_prices
            .entry(window.market_id.clone())
            .or_insert(open_price);
    }

    state.current_window = Some(window.clone());
}

/// Returns the best available market-open price for one live window.
fn recover_window_open_price(
    db: &Database,
    state: &LiveState,
    window: &MarketWindow,
) -> Option<f64> {
    match db.earliest_binance_price_in_window(window.start_time, window.end_time) {
        Ok(Some(price)) => Some(price),
        Ok(None) => state.signal_state.binance_price,
        Err(error) => {
            warn!(market_id = %window.market_id, "failed to recover persisted window open price: {error}");
            state.signal_state.binance_price
        }
    }
}

/// Persists and logs every pending rejection summary for all active markets.
fn flush_all_rejection_summaries(
    db: &Database,
    tracker: &mut StrategyRejectionTracker,
    timestamp_ms: u64,
) {
    let rows = tracker.drain_all(timestamp_ms);
    persist_rejection_summary_rows(db, &rows);
}

/// Persists and logs every pending rejection summary for one market.
fn flush_rejection_summaries_for_market(
    db: &Database,
    tracker: &mut StrategyRejectionTracker,
    market_id: &str,
    timestamp_ms: u64,
) {
    let rows = tracker.drain_market(market_id, timestamp_ms);
    persist_rejection_summary_rows(db, &rows);
}

/// Writes rejection summaries to `SQLite` and mirrors them into structured logs.
fn persist_rejection_summary_rows(
    db: &Database,
    rows: &[crate::types::StrategyRejectionSummaryRecord],
) {
    if rows.is_empty() {
        return;
    }

    for row in rows {
        if let Err(error) = db.log_strategy_rejection_summary(row) {
            error!(
                market_id = %row.market_id,
                strategy = %row.strategy,
                reason = %row.reason,
                "failed to persist strategy rejection summary: {error}"
            );
        }
    }

    log_rejection_rollups(rows);
}

/// Track one weighted mean across already-aggregated rejection summaries.
#[derive(Default)]
struct WeightedMetric {
    sum: f64,
    weight: u64,
}

impl WeightedMetric {
    /// Record one optional value together with the number of evaluations it represents.
    fn record(&mut self, value: Option<f64>, weight: u64) {
        if let Some(value) = value {
            self.sum += value * weight as f64;
            self.weight += weight;
        }
    }

    /// Return the weighted arithmetic mean when at least one sample was recorded.
    fn mean(&self) -> Option<f64> {
        if self.weight == 0 {
            return None;
        }
        Some(self.sum / self.weight as f64)
    }
}

/// Aggregate all rejection reasons and numeric means for one market/strategy pair.
#[derive(Default)]
struct RejectionRollup {
    market_id: String,
    strategy: String,
    total_count: u64,
    reasons: std::collections::HashMap<String, u64>,
    quote_age_ms: WeightedMetric,
    book_staleness_ms: WeightedMetric,
    up_ask: WeightedMetric,
    down_ask: WeightedMetric,
    total_ask: WeightedMetric,
    distance_from_open_bps: WeightedMetric,
    realized_vol_15s_bps: WeightedMetric,
    distance_vol_ratio: WeightedMetric,
    open_crosses_30s: WeightedMetric,
    alignment_fraction: WeightedMetric,
    quote_churn_per_s: WeightedMetric,
    move_velocity: WeightedMetric,
}

/// Human-readable rejection rollup emitted into the operator log.
#[derive(Debug, PartialEq, Eq)]
struct FormattedRejectionRollup {
    market_id: String,
    strategy: String,
    total_count: u64,
    reason_summary: String,
    metrics_summary: String,
}

/// Build concise operator-facing rejection rollups from persisted summaries.
#[allow(clippy::too_many_lines)]
fn build_rejection_rollups(
    rows: &[crate::types::StrategyRejectionSummaryRecord],
) -> Vec<FormattedRejectionRollup> {
    if rows.is_empty() {
        return Vec::new();
    }

    let mut rollups: std::collections::HashMap<(String, String), RejectionRollup> =
        std::collections::HashMap::new();
    for row in rows {
        let details = match serde_json::from_str::<serde_json::Value>(&row.details_json) {
            Ok(details) => details,
            Err(error) => {
                warn!(
                    market_id = %row.market_id,
                    strategy = %row.strategy,
                    reason = %row.reason,
                    "failed to parse rejection summary details for rollup: {error}"
                );
                continue;
            }
        };
        let mean = details.get("mean").cloned().unwrap_or_default();
        let entry = rollups
            .entry((row.market_id.clone(), row.strategy.clone()))
            .or_insert_with(|| RejectionRollup {
                market_id: row.market_id.clone(),
                strategy: row.strategy.clone(),
                ..RejectionRollup::default()
            });
        entry.total_count += row.count;
        *entry.reasons.entry(row.reason.clone()).or_insert(0) += row.count;
        entry
            .quote_age_ms
            .record(json_u64_as_f64(mean.get("quoteAgeMs")), row.count);
        entry
            .book_staleness_ms
            .record(json_u64_as_f64(mean.get("bookStalenessMs")), row.count);
        entry.up_ask.record(json_f64(mean.get("upAsk")), row.count);
        entry
            .down_ask
            .record(json_f64(mean.get("downAsk")), row.count);
        entry
            .total_ask
            .record(json_f64(mean.get("totalAsk")), row.count);
        entry
            .distance_from_open_bps
            .record(json_f64(mean.get("distanceFromOpenBps")), row.count);
        entry
            .realized_vol_15s_bps
            .record(json_f64(mean.get("realizedVol15sBps")), row.count);
        entry
            .distance_vol_ratio
            .record(json_f64(mean.get("distanceVolRatio")), row.count);
        entry
            .open_crosses_30s
            .record(json_u64_as_f64(mean.get("openCrosses30s")), row.count);
        entry
            .alignment_fraction
            .record(json_f64(mean.get("alignmentFraction")), row.count);
        entry
            .quote_churn_per_s
            .record(json_f64(mean.get("quoteChurnPerS")), row.count);
        entry
            .move_velocity
            .record(json_f64(mean.get("moveVelocity")), row.count);
    }

    let mut rollups = rollups.into_values().collect::<Vec<_>>();
    rollups.sort_by(|left, right| {
        left.market_id
            .cmp(&right.market_id)
            .then_with(|| left.strategy.cmp(&right.strategy))
    });

    rollups
        .into_iter()
        .map(|rollup| {
            let mut reasons = rollup.reasons.into_iter().collect::<Vec<_>>();
            reasons.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            let reason_summary = reasons
                .into_iter()
                .take(3)
                .map(|(reason, count)| {
                    format!(
                        "{reason}={:.1}%",
                        (count as f64 / rollup.total_count.max(1) as f64) * 100.0
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut metrics = vec![
                format!(
                    "quoteAgeMs={}",
                    format_optional_u64(rollup.quote_age_ms.mean())
                ),
                format!(
                    "bookStalenessMs={}",
                    format_optional_u64(rollup.book_staleness_ms.mean())
                ),
                format!("upAsk={}", format_optional_f64(rollup.up_ask.mean(), 3)),
                format!("downAsk={}", format_optional_f64(rollup.down_ask.mean(), 3)),
                format!(
                    "totalAsk={}",
                    format_optional_f64(rollup.total_ask.mean(), 3)
                ),
            ];
            append_metric(
                &mut metrics,
                "distanceFromOpenBps",
                rollup.distance_from_open_bps.mean(),
                2,
            );
            append_metric(
                &mut metrics,
                "realizedVol15sBps",
                rollup.realized_vol_15s_bps.mean(),
                2,
            );
            append_metric(
                &mut metrics,
                "distanceVolRatio",
                rollup.distance_vol_ratio.mean(),
                2,
            );
            append_integer_metric(
                &mut metrics,
                "openCrosses30s",
                rollup.open_crosses_30s.mean(),
            );
            append_metric(
                &mut metrics,
                "alignmentFraction",
                rollup.alignment_fraction.mean(),
                2,
            );
            metrics.push(format!(
                "quoteChurnPerS={}",
                format_optional_f64(rollup.quote_churn_per_s.mean(), 1)
            ));
            metrics.push(format!(
                "moveVelocity={}",
                format_optional_f64(rollup.move_velocity.mean(), 6)
            ));
            let metrics_summary = metrics.join(" ");
            FormattedRejectionRollup {
                market_id: rollup.market_id,
                strategy: rollup.strategy,
                total_count: rollup.total_count,
                reason_summary,
                metrics_summary,
            }
        })
        .collect()
}

/// Emit concise operator-facing rejection rollups for one batch of summaries.
fn log_rejection_rollups(rows: &[crate::types::StrategyRejectionSummaryRecord]) {
    for rollup in build_rejection_rollups(rows) {
        info!(
            market_id = %rollup.market_id,
            strategy = %rollup.strategy,
            evaluations = rollup.total_count,
            top_reasons = %rollup.reason_summary,
            metrics = %rollup.metrics_summary,
            "strategy rejection rollup"
        );
    }
}

/// Process all due paper orders at the current live timestamp and emit concise outcome logs.
#[allow(clippy::too_many_arguments)]
fn process_due_orders_and_log(
    execution_engine: &mut ExecutionEngine,
    current_ms: u64,
    current_micros: Option<u64>,
    current_window: Option<&MarketWindow>,
    book_state: &crate::types::BookState,
    db: &Database,
    bankroll: &mut BankrollManager,
    config: &Config,
    clock: &SystemClock,
) {
    match execution_engine.process_due_orders(
        current_ms,
        current_micros,
        current_window,
        book_state,
        db,
        bankroll,
        config,
        clock,
    ) {
        Ok(_) => log_processed_order_outcomes(execution_engine.take_recent_outcomes()),
        Err(error) => error!("failed to process due paper orders: {error}"),
    }
}

/// Emit one concise operator-facing log line for each processed paper order.
fn log_processed_order_outcomes(outcomes: Vec<ProcessedOrderOutcome>) {
    for outcome in outcomes {
        match outcome.disposition {
            OrderOutcomeDisposition::Filled => {
                info!(
                    signal_id = outcome.signal_id,
                    market_id = %outcome.market_id,
                    strategy = %outcome.strategy,
                    side = %outcome.side,
                    best_ask = outcome.best_ask.unwrap_or_default(),
                    ask_size = outcome.ask_size.unwrap_or_default(),
                    freshness_ms = outcome.freshness_ms.unwrap_or_default(),
                    requested_size = outcome.requested_size,
                    filled_size = outcome.filled_size,
                    effective_arrival_delay_ms = outcome.effective_arrival_delay_ms,
                    partial_fill = outcome.partial_fill,
                    "paper order filled"
                );
            }
            OrderOutcomeDisposition::Missed => {
                info!(
                    signal_id = outcome.signal_id,
                    market_id = %outcome.market_id,
                    strategy = %outcome.strategy,
                    side = %outcome.side,
                    reason = %outcome.reason.as_deref().unwrap_or("unknown"),
                    best_ask = outcome.best_ask.unwrap_or_default(),
                    ask_size = outcome.ask_size.unwrap_or_default(),
                    freshness_ms = outcome.freshness_ms.unwrap_or_default(),
                    requested_size = outcome.requested_size,
                    effective_arrival_delay_ms = outcome.effective_arrival_delay_ms,
                    "paper order missed"
                );
            }
        }
    }
}

/// Emit one concise market-close execution rollup from persisted signal metrics.
fn log_execution_rollup_for_market(db: &Database, market_id: &str) {
    match db.execution_rollup_for_market(market_id) {
        Ok(rollup) => {
            let miss_reasons = if rollup.miss_reasons.is_empty() {
                "none".to_string()
            } else {
                rollup
                    .miss_reasons
                    .into_iter()
                    .map(|(reason, count)| format!("{reason}={count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let queue_rejection_reasons = if rollup.queue_rejection_reasons.is_empty() {
                "none".to_string()
            } else {
                rollup
                    .queue_rejection_reasons
                    .into_iter()
                    .map(|(reason, count)| format!("{reason}={count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            info!(
                market_id,
                submitted = rollup.submitted,
                filled = rollup.filled,
                missed = rollup.missed,
                rejected_before_queue = rollup.rejected_before_queue,
                partial = rollup.partial,
                mean_effective_arrival_delay_ms =
                    format_optional_u64(rollup.mean_effective_arrival_delay_ms),
                top_miss_reasons = miss_reasons,
                top_queue_rejection_reasons = queue_rejection_reasons,
                "paper execution rollup"
            );
        }
        Err(error) => warn!(market_id, "failed to build paper execution rollup: {error}"),
    }
}

/// Return one optional numeric value from a JSON summary node.
fn json_f64(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
}

/// Return one optional integer-like JSON value as a floating-point sample.
fn json_u64_as_f64(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(|value| value.as_u64().map(|value| value as f64))
}

/// Format one optional floating-point metric for concise operator logs.
fn format_optional_f64(value: Option<f64>, precision: usize) -> String {
    value.map_or_else(|| "na".to_string(), |value| format!("{value:.precision$}"))
}

/// Format one optional millisecond metric for concise operator logs.
fn format_optional_u64(value: Option<f64>) -> String {
    value.map_or_else(
        || "na".to_string(),
        |value| format!("{}", value.round() as u64),
    )
}

/// Append one optional floating-point metric to a concise rollup string.
fn append_metric(metrics: &mut Vec<String>, label: &str, value: Option<f64>, precision: usize) {
    if let Some(value) = value {
        metrics.push(format!("{label}={value:.precision$}"));
    }
}

/// Append one optional integer-like metric to a concise rollup string.
fn append_integer_metric(metrics: &mut Vec<String>, label: &str, value: Option<f64>) {
    if let Some(value) = value {
        metrics.push(format!("{label}={}", value.round() as u64));
    }
}

/// Insert one persisted feed row and update storage counters on success.
fn persist_feed_event(
    db: &Database,
    storage_state: &mut FeedEventStorageState,
    event: &FeedEvent,
) -> anyhow::Result<()> {
    db.log_feed_event(event)?;
    storage_state.record_persisted(event);
    Ok(())
}

/// Carry the fields that vary between materialized per-side `CLOB` rows.
struct LiveClobRowSpec {
    source: &'static str,
    asset_id: Option<String>,
    market_id: Option<String>,
    payload_json: Option<String>,
    details_json: Option<String>,
}

/// Build the relevant per-side `CLOB` rows for one live top-of-book event.
fn build_live_clob_events(event: &LiveClobLogEvent<'_>) -> Vec<FeedEvent> {
    let mut events = Vec::new();
    let current_market_id = event.current_window.map(|window| window.market_id.clone());
    let payload_json = event.payload_json.map(ToString::to_string);
    let details_json = event.details_json.map(ToString::to_string);

    if let Some(window) = event.current_window {
        if let Some(asset_id) = event.asset_id {
            if asset_id == window.up_token_id {
                push_live_clob_event(
                    &mut events,
                    event,
                    event.book_state.up.as_ref(),
                    LiveClobRowSpec {
                        source: "clob_up",
                        asset_id: Some(window.up_token_id.clone()),
                        market_id: current_market_id,
                        payload_json,
                        details_json,
                    },
                );
                return events;
            }
            if asset_id == window.down_token_id {
                push_live_clob_event(
                    &mut events,
                    event,
                    event.book_state.down.as_ref(),
                    LiveClobRowSpec {
                        source: "clob_down",
                        asset_id: Some(window.down_token_id.clone()),
                        market_id: current_market_id,
                        payload_json,
                        details_json,
                    },
                );
                return events;
            }
        }
        push_live_clob_event(
            &mut events,
            event,
            event.book_state.up.as_ref(),
            LiveClobRowSpec {
                source: "clob_up",
                asset_id: Some(window.up_token_id.clone()),
                market_id: current_market_id.clone(),
                payload_json: payload_json.clone(),
                details_json: details_json.clone(),
            },
        );
        push_live_clob_event(
            &mut events,
            event,
            event.book_state.down.as_ref(),
            LiveClobRowSpec {
                source: "clob_down",
                asset_id: Some(window.down_token_id.clone()),
                market_id: current_market_id,
                payload_json,
                details_json,
            },
        );
        return events;
    }

    push_live_clob_event(
        &mut events,
        event,
        event.book_state.up.as_ref(),
        LiveClobRowSpec {
            source: "clob_up",
            asset_id: event.asset_id.map(str::to_string),
            market_id: None,
            payload_json: payload_json.clone(),
            details_json: details_json.clone(),
        },
    );
    push_live_clob_event(
        &mut events,
        event,
        event.book_state.down.as_ref(),
        LiveClobRowSpec {
            source: "clob_down",
            asset_id: event.asset_id.map(str::to_string),
            market_id: None,
            payload_json,
            details_json,
        },
    );
    events
}

/// Append one materialized `CLOB` row when the corresponding book side exists.
fn push_live_clob_event(
    events: &mut Vec<FeedEvent>,
    context: &LiveClobLogEvent<'_>,
    book: Option<&crate::types::TopOfBook>,
    spec: LiveClobRowSpec,
) {
    let Some(book) = book else {
        return;
    };
    events.push(FeedEvent {
        id: None,
        received_at_ms: context.receive_ms,
        event_at_ms: context.event_ms,
        received_at_us: context.receive_micros,
        event_at_us: context.event_micros,
        source: spec.source.to_string(),
        event_type: context.event_type.to_string(),
        source_topic: context.source_topic.map(str::to_string),
        source_symbol: None,
        connection_id: Some(context.connection_id.to_string()),
        sequence_key: None,
        market_id: spec.market_id,
        asset_id: spec.asset_id,
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(book.best_bid),
        best_ask: Some(book.best_ask),
        bid_size: Some(book.bid_size),
        ask_size: Some(book.ask_size),
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: None,
        payload_json: spec.payload_json,
        details_json: spec.details_json,
        fidelity: ReplayFidelity::RawEvent,
    });
}

/// Logs live clob event.
fn log_live_clob_event(
    db: &Database,
    storage_state: &mut FeedEventStorageState,
    event: &LiveClobLogEvent<'_>,
) -> anyhow::Result<()> {
    for feed_event in build_live_clob_events(event) {
        let prepared = match event.event_type {
            "book" => storage_state.prepare_clob_book_snapshot(feed_event),
            "price_change" | "best_bid_ask" => storage_state.prepare_clob_top_of_book(feed_event),
            _ => Some(feed_event),
        };
        if let Some(feed_event) = prepared {
            persist_feed_event(db, storage_state, &feed_event)?;
        }
    }
    Ok(())
}

/// Record one feed lifecycle or health event in the database.
fn log_feed_health_event(db: &Database, event: &FeedHealthLogEvent<'_>) -> anyhow::Result<()> {
    db.log_feed_health_event(&FeedHealthEvent {
        id: None,
        timestamp_ms: event.timestamp_ms,
        timestamp_us: event.timestamp_micros,
        source: event.source.to_string(),
        event_type: event.event_type.to_string(),
        connection_id: event.connection_id.map(str::to_string),
        market_id: event.market_id.map(str::to_string),
        details_json: event.details_json.map(str::to_string),
    })?;
    Ok(())
}

/// Emit concise operator-facing feed-health rollups for recent disconnect activity.
fn log_feed_health_rollups(rows: &[FeedHealthRollupRow]) {
    for row in rows {
        let cause_summary = if row.cause_counts.is_empty() {
            "none".to_string()
        } else {
            row.cause_counts
                .iter()
                .map(|(cause, count)| format!("{cause}={count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let active_outage_s = row.active_outage_ms.map_or_else(
            || "none".to_string(),
            |ms| format!("{:.1}", ms as f64 / 1000.0),
        );
        let active_cause = row.active_cause_class.as_deref().unwrap_or("none");

        info!(
            feed = %row.source,
            disconnects = row.disconnect_count,
            cumulative_downtime_s = format!("{:.1}", row.cumulative_downtime_ms as f64 / 1000.0),
            max_downtime_s = format!("{:.1}", row.max_downtime_ms as f64 / 1000.0),
            active_outage_s,
            active_cause,
            causes = %cause_summary,
            "feed health rollup"
        );
    }
}

/// Return the initial quality class implied by the configured storage profile.
fn initial_replay_quality_class(profile: FeedEventStorageProfile) -> &'static str {
    match profile {
        FeedEventStorageProfile::Compact => "descriptive_only",
        FeedEventStorageProfile::ReplayGrade | FeedEventStorageProfile::FullDebug => "sweep_grade",
    }
}

/// Return the required feed classes for sweep-grade replay.
fn required_feed_event_classes() -> &'static str {
    "binance:aggTrade, binance:bookTicker, binance:depth, chainlink:chainlink_price, clob_up:top_of_book, clob_down:top_of_book"
}

/// Return the best current quality class implied by persisted feed classes.
fn footprint_replay_quality_class(rows: &[FeedEventFootprintRow]) -> &'static str {
    if has_feed_class(rows, "binance", "aggTrade")
        && has_feed_class(rows, "binance", "bookTicker")
        && has_feed_class(rows, "binance", "depth")
        && has_feed_class(rows, "chainlink", "chainlink_price")
        && has_source(rows, "clob_up")
        && has_source(rows, "clob_down")
    {
        "sweep_grade"
    } else if rows.is_empty() {
        "empty"
    } else {
        "descriptive_only"
    }
}

/// Return whether footprint rows include one source and event type.
fn has_feed_class(rows: &[FeedEventFootprintRow], source: &str, event_type: &str) -> bool {
    rows.iter()
        .any(|row| row.source == source && row.event_type == event_type && row.row_count > 0)
}

/// Return whether footprint rows include any event for one source.
fn has_source(rows: &[FeedEventFootprintRow], source: &str) -> bool {
    rows.iter()
        .any(|row| row.source == source && row.row_count > 0)
}

/// Register or update one market that still needs authoritative settlement.
fn schedule_pending_resolution(
    pending_resolutions: &mut std::collections::HashMap<String, PendingResolution>,
    window: &MarketWindow,
    next_attempt_at_ms: u64,
    seeded_from_startup: bool,
) {
    use std::collections::hash_map::Entry;

    match pending_resolutions.entry(window.market_id.clone()) {
        Entry::Occupied(mut entry) => {
            let pending = entry.get_mut();
            pending.next_attempt_at_ms = pending.next_attempt_at_ms.min(next_attempt_at_ms);
            pending.seeded_from_startup &= seeded_from_startup;
        }
        Entry::Vacant(entry) => {
            entry.insert(PendingResolution {
                market_id: window.market_id.clone(),
                window: window.clone(),
                next_attempt_at_ms,
                seeded_from_startup,
            });
        }
    }
}

/// Seed the durable authoritative-settlement registry from unresolved open trades.
fn seed_pending_resolutions(
    db: &Database,
    config: &Config,
    clock: &dyn Clock,
) -> std::collections::HashMap<String, PendingResolution> {
    let now = clock.now();
    let unresolved = match db.unresolved_open_trade_markets(now) {
        Ok(markets) => markets,
        Err(e) => {
            warn!("failed to load unresolved open-trade markets for reconciliation: {e}");
            return std::collections::HashMap::new();
        }
    };

    let mut pending_resolutions = std::collections::HashMap::new();
    for market in unresolved {
        if let Err(e) = db.resolve_market(&market.window.market_id, "closed") {
            warn!(
                market_id = %market.window.market_id,
                "failed to normalize unresolved market to closed on startup: {e}"
            );
        }
        let next_attempt_at_ms = market
            .window
            .end_time
            .saturating_add(config.resolution_initial_delay_ms)
            .max(now);
        schedule_pending_resolution(
            &mut pending_resolutions,
            &market.window,
            next_attempt_at_ms,
            true,
        );
    }

    if !pending_resolutions.is_empty() {
        info!(
            pending_resolution_markets = pending_resolutions.len(),
            "seeded authoritative settlement reconciliation from unresolved open trades"
        );
    }

    pending_resolutions
}

/// Remove and return pending authoritative settlements whose next attempt is due.
fn take_ready_pending_resolutions(
    pending_resolutions: &mut std::collections::HashMap<String, PendingResolution>,
    now: u64,
) -> Vec<PendingResolution> {
    let ready_market_ids = pending_resolutions
        .iter()
        .filter(|(_, pending)| pending.next_attempt_at_ms <= now)
        .map(|(market_id, _)| market_id.clone())
        .collect::<Vec<_>>();

    let mut ready = Vec::with_capacity(ready_market_ids.len());
    for market_id in ready_market_ids {
        if let Some(pending) = pending_resolutions.remove(&market_id) {
            ready.push(pending);
        }
    }
    ready
}

/// Apply one authoritative resolution exactly once for the closed window.
#[allow(clippy::too_many_arguments)]
fn handle_authoritative_resolution(
    window: &MarketWindow,
    authoritative_outcome: SignalDirection,
    seeded_from_startup: bool,
    db: &Database,
    position_manager: &mut PositionManager,
    bankroll: &mut BankrollManager,
    trend_tracker: &mut ScopedTrendTracker,
    circuit_breaker: &mut CircuitBreaker,
    config: &Config,
    clock: &dyn Clock,
) {
    let now = clock.now();
    let auth_str = authoritative_outcome.to_string();

    let resolved = position_manager.resolve_window_with_outcome(
        window,
        authoritative_outcome,
        db,
        bankroll,
        config,
        clock,
    );

    if seeded_from_startup && !resolved.is_empty() {
        info!(
            market_id = %window.market_id,
            settled_trades = resolved.len(),
            "startup reconciliation backfilled unresolved trade settlements"
        );
    }

    for (trade, result) in &resolved {
        let won = result.pnl_net > 0.0;
        trend_tracker.record_outcome(family_for_strategy(&trade.strategy), trade.side, won, now);
        circuit_breaker.record_result(won, now);
        if let Some(trade_id) = trade.id {
            let prediction = trade.side.to_string();
            let _ =
                db.log_settlement_audit(trade_id, &window.market_id, &prediction, &auth_str, now);
            info!(
                trade_id,
                strategy = %trade.strategy,
                side = %trade.side,
                pnl_net = result.pnl_net,
                fee = result.fee_amount,
                auth_outcome = auth_str,
                "trade settled from authoritative Polymarket resolution"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExecutionMode;
    use tempfile::NamedTempFile;

    /// Verifies that persisted Binance ticks take precedence when recovering a window open.
    #[test]
    fn recover_window_open_price_prefers_persisted_tick() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
        db.log_tick(1_100, "binance", Some(42_100.0), None, None, None, None)
            .unwrap();
        db.log_tick(1_200, "binance", Some(42_200.0), None, None, None, None)
            .unwrap();

        let mut state = LiveState::new();
        state.signal_state.binance_price = Some(43_000.0);
        let window = MarketWindow {
            market_id: "mkt-1".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "up".to_string(),
            down_token_id: "down".to_string(),
            condition_id: "cond-1".to_string(),
            start_time: 1_000,
            end_time: 2_000,
            slug: "btc-updown-5m-1".to_string(),
            outcome: None,
            resolution_source: None,
            fee_profile: None,
            order_min_size: None,
            order_price_min_tick_size: None,
            maker_base_fee: None,
            taker_base_fee: None,
            rewards_min_size: None,
            rewards_max_spread: None,
            fees_enabled: None,
            fee_schedule_json: None,
            token_fee_rates_json: None,
            accepting_orders: None,
            accepting_orders_timestamp: None,
            clear_book_on_start: None,
        };

        let open_price = recover_window_open_price(&db, &state, &window);
        assert_eq!(open_price, Some(42_100.0));
    }

    /// Verifies that the latest in-memory Binance price is used only when no persisted tick exists.
    #[test]
    fn recover_window_open_price_falls_back_to_live_price() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();

        let mut state = LiveState::new();
        state.signal_state.binance_price = Some(43_000.0);
        let window = MarketWindow {
            market_id: "mkt-1".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "up".to_string(),
            down_token_id: "down".to_string(),
            condition_id: "cond-1".to_string(),
            start_time: 1_000,
            end_time: 2_000,
            slug: "btc-updown-5m-1".to_string(),
            outcome: None,
            resolution_source: None,
            fee_profile: None,
            order_min_size: None,
            order_price_min_tick_size: None,
            maker_base_fee: None,
            taker_base_fee: None,
            rewards_min_size: None,
            rewards_max_spread: None,
            fees_enabled: None,
            fee_schedule_json: None,
            token_fee_rates_json: None,
            accepting_orders: None,
            accepting_orders_timestamp: None,
            clear_book_on_start: None,
        };

        let open_price = recover_window_open_price(&db, &state, &window);
        assert_eq!(open_price, Some(43_000.0));
    }

    /// Verifies that operator-facing rejection rollups stay concise while
    /// preserving reason percentages and mean quote context.
    #[test]
    fn build_rejection_rollups_formats_concise_reason_summary() {
        let rows = vec![
            crate::types::StrategyRejectionSummaryRecord {
                timestamp_ms: 1_000,
                market_id: "mkt-1".to_string(),
                strategy: "latency-arb".to_string(),
                reason: "features_stale".to_string(),
                count: 75,
                details_json: serde_json::json!({
                    "last": {"upAsk": 0.50},
                    "mean": {
                        "quoteAgeMs": 120,
                        "bookStalenessMs": 140,
                        "upAsk": 0.51,
                        "downAsk": 0.49,
                        "totalAsk": 1.00,
                        "quoteChurnPerS": 8.0,
                        "moveVelocity": 0.0002
                    }
                })
                .to_string(),
            },
            crate::types::StrategyRejectionSummaryRecord {
                timestamp_ms: 1_000,
                market_id: "mkt-1".to_string(),
                strategy: "latency-arb".to_string(),
                reason: "window_too_late".to_string(),
                count: 25,
                details_json: serde_json::json!({
                    "last": {"upAsk": 0.55},
                    "mean": {
                        "quoteAgeMs": 200,
                        "bookStalenessMs": 240,
                        "upAsk": 0.55,
                        "downAsk": 0.45,
                        "totalAsk": 1.00,
                        "quoteChurnPerS": 4.0,
                        "moveVelocity": 0.0001
                    }
                })
                .to_string(),
            },
        ];

        let rollups = build_rejection_rollups(&rows);
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].market_id, "mkt-1");
        assert_eq!(rollups[0].strategy, "latency-arb");
        assert_eq!(rollups[0].total_count, 100);
        assert_eq!(
            rollups[0].reason_summary,
            "features_stale=75.0%, window_too_late=25.0%"
        );
        assert!(rollups[0].metrics_summary.contains("quoteAgeMs=140"));
        assert!(rollups[0].metrics_summary.contains("bookStalenessMs=165"));
        assert!(!rollups[0].metrics_summary.contains('{'));
    }

    /// Verify that calm-specific rejection metrics are included only when present.
    #[test]
    fn build_rejection_rollups_includes_calm_specific_metrics() {
        let rows = vec![crate::types::StrategyRejectionSummaryRecord {
            timestamp_ms: 1_000,
            market_id: "mkt-2".to_string(),
            strategy: "calm-persistence".to_string(),
            reason: "distance_below_threshold".to_string(),
            count: 10,
            details_json: serde_json::json!({
                "last": {"distanceFromOpenBps": 4.5},
                "mean": {
                    "quoteAgeMs": 5,
                    "bookStalenessMs": 7,
                    "upAsk": 0.61,
                    "downAsk": 0.71,
                    "totalAsk": 1.32,
                    "distanceFromOpenBps": 4.75,
                    "realizedVol15sBps": 6.5,
                    "distanceVolRatio": 0.73,
                    "openCrosses30s": 1,
                    "alignmentFraction": 0.50,
                    "quoteChurnPerS": 12.0,
                    "moveVelocity": 0.00003
                }
            })
            .to_string(),
        }];

        let rollups = build_rejection_rollups(&rows);
        assert_eq!(rollups.len(), 1);
        assert!(
            rollups[0]
                .metrics_summary
                .contains("distanceFromOpenBps=4.75")
        );
        assert!(
            rollups[0]
                .metrics_summary
                .contains("realizedVol15sBps=6.50")
        );
        assert!(rollups[0].metrics_summary.contains("distanceVolRatio=0.73"));
        assert!(rollups[0].metrics_summary.contains("openCrosses30s=1"));
        assert!(
            rollups[0]
                .metrics_summary
                .contains("alignmentFraction=0.50")
        );
    }

    /// Verifies that completed feed outages contribute bounded downtime rollups.
    #[test]
    fn feed_health_tracker_rolls_up_completed_outage() {
        let mut tracker = FeedHealthTracker::default();
        tracker.note_disconnected("clob", "websocket_error", 1_000);
        tracker.note_connected("clob", 1_450);

        let rows = tracker.take_rollups(2_000);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "clob");
        assert_eq!(rows[0].disconnect_count, 1);
        assert_eq!(rows[0].cumulative_downtime_ms, 450);
        assert_eq!(rows[0].max_downtime_ms, 450);
        assert_eq!(rows[0].active_outage_ms, None);
        assert_eq!(rows[0].active_cause_class, None);
        assert_eq!(
            rows[0].cause_counts,
            vec![("websocket_error".to_string(), 1)]
        );
    }

    /// Verifies that ongoing outages stay visible in periodic feed-health rollups.
    #[test]
    fn feed_health_tracker_reports_active_outage() {
        let mut tracker = FeedHealthTracker::default();
        tracker.note_disconnected("binance", "idle_timeout", 2_000);

        let rows = tracker.take_rollups(2_750);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "binance");
        assert_eq!(rows[0].disconnect_count, 1);
        assert_eq!(rows[0].cumulative_downtime_ms, 0);
        assert_eq!(rows[0].max_downtime_ms, 0);
        assert_eq!(rows[0].active_outage_ms, Some(750));
        assert_eq!(rows[0].active_cause_class.as_deref(), Some("idle_timeout"));
        assert_eq!(rows[0].cause_counts, vec![("idle_timeout".to_string(), 1)]);
    }

    /// Verifies that live-trading runtime can start disarmed and shut down locally.
    #[tokio::test]
    async fn run_live_starts_live_trading_disarmed() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.live_sidecar_url = "http://127.0.0.1:9".to_string();
        let tmp_db = NamedTempFile::new().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        shutdown_tx.send(()).unwrap();

        let result = run_live(config, tmp_db.path().to_str().unwrap(), 100.0, shutdown_rx).await;

        assert!(result.is_ok());
    }

    /// Verifies that compact capture blocks live arming.
    #[test]
    fn live_gate_issues_require_replay_grade_capture() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.feed_event_storage_profile = FeedEventStorageProfile::Compact;
        let issues = live_gate_issues(
            &test_preflight_response(),
            &test_account_state(),
            &test_activity_response(),
            &config,
        );

        assert!(issues.iter().any(|issue| issue.contains("replay-grade")));
    }

    /// Verifies that unknown venue outcomes block future live submissions.
    #[test]
    fn live_order_response_blocking_statuses_are_terminal() {
        let response = LiveOrderIntentResponse {
            ok: false,
            venue_order_id: None,
            client_order_id: "client-1".to_string(),
            status: "unknown_submission".to_string(),
            status_reason: Some("timeout".to_string()),
            accepted_size: None,
            details_json: None,
        };

        assert!(live_order_response_is_blocking(&response));
    }

    /// Build one passing preflight fixture for live gate tests.
    fn test_preflight_response() -> LivePreflightResponse {
        LivePreflightResponse {
            ok: true,
            mode: "live_trading".to_string(),
            wallet_address: Some("0xwallet".to_string()),
            proxy_wallet: Some("0xproxy".to_string()),
            geoblock_status: LiveCheckStatus::Ok,
            geoblock_country_code: Some("IE".to_string()),
            auth_status: LiveCheckStatus::Ok,
            clock_status: LiveCheckStatus::Ok,
            allowance_status: LiveCheckStatus::Ok,
            user_stream_status: LiveCheckStatus::Ok,
            available_cash_usd: Some(100.0),
            legal_order_min_usd: Some(5.0),
            details_json: None,
            errors: Vec::new(),
        }
    }

    /// Build one passing account fixture for live gate tests.
    fn test_account_state() -> LiveAccountState {
        LiveAccountState {
            timestamp_ms: 1_000,
            wallet_address: Some("0xwallet".to_string()),
            proxy_wallet: Some("0xproxy".to_string()),
            cash_available: 100.0,
            cash_reserved_for_orders: 0.0,
            inventory_mark_value: 0.0,
            redeemable_value: 0.0,
            pending_redeem_value: 0.0,
            total_equity: 100.0,
            allowance_available: Some(100.0),
            details_json: None,
        }
    }

    /// Build one passing activity fixture for live gate tests.
    fn test_activity_response() -> LiveActivityResponse {
        LiveActivityResponse {
            timestamp_ms: 1_000,
            user_stream_status: LiveCheckStatus::Ok,
            user_stream_events: Vec::new(),
            clob_trades: Vec::new(),
            details_json: None,
        }
    }
}

/// Evaluate strategies.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn evaluate_strategies(
    state: &mut LiveState,
    momentum: &MomentumCalculator,
    config: &Config,
    clock: &SystemClock,
    db: &Database,
    strategies: &mut [Box<dyn Strategy>],
    execution_engine: &mut ExecutionEngine,
    bankroll: &mut BankrollManager,
    circuit_breaker: &mut CircuitBreaker,
    trend_tracker: &mut ScopedTrendTracker,
    rejection_tracker: &mut StrategyRejectionTracker,
    live_trading_can_submit: bool,
    now: u64,
) -> Vec<QueuedOrderIntent> {
    let Some(window) = state.current_window.as_ref() else {
        return Vec::new();
    };
    let Some(binance_price) = state.signal_state.binance_price else {
        return Vec::new();
    };

    if !circuit_breaker.can_trade(now) {
        circuit_breaker.log_if_paused(now);
        return Vec::new();
    }
    if config.execution_mode == crate::config::ExecutionMode::LiveTrading
        && !live_trading_can_submit
    {
        return Vec::new();
    }

    let window_open_price = state.window_open_prices.get(&window.market_id).copied();
    let now_us = Some(now_us());
    let features = SignalFeatureEngine::compute(
        &mut state.signal_state,
        Some(window),
        window_open_price,
        momentum.get(),
        now,
        now_us,
        config,
    );

    let ctx = StrategyContext {
        binance_price,
        binance_momentum: momentum.get(),
        chainlink_price: state.signal_state.chainlink_price,
        book_state: state.signal_state.book_state.clone(),
        window_open_price,
        window_time_remaining_ms: window.end_time.saturating_sub(now),
        now_us,
        features,
    };

    let mut live_orders = Vec::new();
    match run_strategy_cycle(
        &ctx,
        window,
        config,
        clock,
        db,
        strategies,
        execution_engine,
        bankroll,
        trend_tracker,
        rejection_tracker,
        ReplayFidelity::RawEvent,
        now,
    ) {
        Ok(outcome) => {
            for event in outcome.events {
                match event {
                    StrategyCycleEvent::Suppressed {
                        strategy,
                        direction,
                        regime,
                    } => {
                        info!(
                            strategy = %strategy,
                            direction = %direction,
                            regime = regime.as_str(),
                            "signal suppressed by trend filter"
                        );
                    }
                    StrategyCycleEvent::SingleSubmitted {
                        strategy,
                        direction,
                        regime,
                        outcome,
                    } => match outcome {
                        SubmissionOutcome::Queued { signal_ids, orders } => {
                            info!(
                                signal_id = signal_ids.first().copied().unwrap_or_default(),
                                strategy = %strategy,
                                direction = %direction,
                                regime = regime.as_str(),
                                "signal queued"
                            );
                            if config.execution_mode == crate::config::ExecutionMode::LiveTrading {
                                live_orders.extend(orders);
                            }
                        }
                        SubmissionOutcome::Rejected { signal_ids, reason } => {
                            info!(
                                signal_id = signal_ids.first().copied().unwrap_or_default(),
                                strategy = %strategy,
                                direction = %direction,
                                reason = %reason,
                                regime = regime.as_str(),
                                "signal rejected before queue"
                            );
                        }
                    },
                    StrategyCycleEvent::BatchSubmitted {
                        strategy,
                        count,
                        regime,
                        outcome,
                    } => match outcome {
                        SubmissionOutcome::Queued { signal_ids, orders } => {
                            info!(
                                strategy = %strategy,
                                count = signal_ids.len().max(count),
                                regime = regime.as_str(),
                                "batch queued"
                            );
                            if config.execution_mode == crate::config::ExecutionMode::LiveTrading {
                                live_orders.extend(orders);
                            }
                        }
                        SubmissionOutcome::Rejected { signal_ids, reason } => {
                            info!(
                                strategy = %strategy,
                                count = signal_ids.len().max(count),
                                reason = %reason,
                                regime = regime.as_str(),
                                "batch rejected before queue"
                            );
                        }
                    },
                }
            }
        }
        Err(error) => {
            error!("failed to evaluate strategies: {error}");
        }
    }
    live_orders
}
