use std::sync::Arc;

use tracing::{error, info, warn};

use crate::backtest::momentum::MomentumCalculator;
use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::db::database::Database;
use crate::executor::{
    ExecutionEngine, OrderOutcomeDisposition, ProcessedOrderOutcome, SubmissionOutcome,
};
use crate::feeds::FeedMessage;
use crate::feeds::util::now_us;
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
    FeedEvent, FeedHealthEvent, MarketWindow, ReplayFidelity, SignalDirection, StrategyContext,
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

/// Run the live paper trading bot.
///
/// The bot runs until `shutdown_rx` fires, at which point it performs a
/// graceful shutdown (logs final stats, closes the database).
///
/// In the CLI binary, `shutdown_rx` is wired to `tokio::signal::ctrl_c()`.
/// In tests, it is fired by the test harness after a scripted scenario.
#[allow(clippy::too_many_lines)]
pub async fn run_live(
    config: Config,
    db_path: &str,
    balance: f64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    config.validate()?;
    let pending_policy = config.pending_settlement_policy_unchecked();
    info!(
        balance = balance,
        db = %db_path,
        pending_settlement_mode = pending_policy.mode.as_str(),
        pending_settlement_family_reserve_fraction = pending_policy.family_reserve_fraction,
        pending_settlement_global_reserve_fraction = pending_policy.global_reserve_fraction,
        pending_settlement_counts_as_open_position = pending_policy.counts_as_open_position,
        "starting live paper trading bot"
    );

    let db = Database::new(db_path)?;

    let clock = SystemClock;
    let mut bankroll = BankrollManager::new(balance, &config, &db, &clock);
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
    let mut rejection_tracker = StrategyRejectionTracker::new();
    let mut pending_resolutions = seed_pending_resolutions(&db, &config, &clock);

    let (activate_tx, mut activate_rx) = tokio::sync::mpsc::channel::<MarketWindow>(32);
    let mut storage_report_timer = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
    storage_report_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let _ = storage_report_timer.tick().await;
    let mut resolution_retry_timer = tokio::time::interval(std::time::Duration::from_secs(1));
    resolution_retry_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                        evaluate_strategies(
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
                            receive.ms,
                        );
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

                    FeedMessage::FeedDisconnected { name, connection_id } => {
                        warn!(feed = %name, "feed disconnected");
                        let _ = log_feed_health_event(
                            &db,
                            &FeedHealthLogEvent {
                                timestamp_ms: clock.now(),
                                timestamp_micros: Some(now_us()),
                                source: &name,
                                event_type: "disconnected",
                                connection_id: connection_id.as_deref(),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.as_str()),
                                details_json: None,
                            },
                        );
                    }

                    FeedMessage::ChainlinkStale { connection_id } => {
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
                                details_json: None,
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
                        "live storage footprint"
                    );
                }
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

    db.close();
    info!("database closed, goodbye");

    Ok(())
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
    now: u64,
) {
    let Some(window) = state.current_window.as_ref() else {
        return;
    };
    let Some(binance_price) = state.signal_state.binance_price else {
        return;
    };

    if !circuit_breaker.can_trade(now) {
        circuit_breaker.log_if_paused(now);
        return;
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
                        SubmissionOutcome::Queued { signal_ids } => {
                            info!(
                                signal_id = signal_ids.first().copied().unwrap_or_default(),
                                strategy = %strategy,
                                direction = %direction,
                                regime = regime.as_str(),
                                "signal queued"
                            );
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
                        SubmissionOutcome::Queued { signal_ids } => {
                            info!(
                                strategy = %strategy,
                                count = signal_ids.len().max(count),
                                regime = regime.as_str(),
                                "batch queued"
                            );
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
}
