use std::sync::Arc;

use tracing::{error, info, warn};

use crate::backtest::momentum::MomentumCalculator;
use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::db::database::Database;
use crate::executor::ExecutionEngine;
use crate::feeds::FeedMessage;
use crate::feeds::util::now_us;
use crate::live_storage::FeedEventStorageState;
use crate::market_discovery::{self, MarketDiscoveryEvent};
use crate::position_manager::PositionManager;
use crate::signal_features::{SignalFeatureEngine, SignalState};
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategies::{Strategy, StrategyResult};
use crate::tick_logger::{self, TickLoggerState};
use crate::trend_tracker::TrendTracker;
use crate::types::{
    FeedEvent, FeedHealthEvent, MarketWindow, ReplayFidelity, SignalDirection, StrategyContext,
};

struct DeferredResolution {
    market_id: String,
    window: MarketWindow,
    authoritative_outcome: SignalDirection,
}

struct LiveState {
    signal_state: SignalState,
    current_window: Option<MarketWindow>,
    window_open_prices: std::collections::HashMap<String, f64>,
    known_windows: std::collections::HashMap<String, MarketWindow>,
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
    info!(
        balance = balance,
        db = %db_path,
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
    let mut trend_tracker = TrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
    );

    let mut strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(LatencyArbStrategy::new(
            config.latency_arb_momentum_threshold,
        )),
        Box::new(SpreadCaptureStrategy::new()),
    ];

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

    let (resolution_tx, mut resolution_rx) = tokio::sync::mpsc::channel::<DeferredResolution>(32);

    let (activate_tx, mut activate_rx) = tokio::sync::mpsc::channel::<MarketWindow>(32);
    let mut storage_report_timer = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
    storage_report_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let _ = storage_report_timer.tick().await;

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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                        let _ = execution_engine.process_due_orders(
                            receive.ms,
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

                            activate_window(&mut state, &window, &clob_handle);
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
                            let open = state.window_open_prices.remove(&closed_id).unwrap_or_else(|| {
                                warn!(market_id = %closed_id, "no open price captured, using current Binance price");
                                state.signal_state.binance_price.unwrap_or(0.0)
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

                            if db
                                .get_open_trades_for_market(&closed_id)
                                .is_ok_and(|trades| !trades.is_empty())
                            {
                                let tx = resolution_tx.clone();
                                let gamma_url = config.gamma_api_url.clone();
                                let slug = window.slug.clone();
                                let mid = window.market_id.clone();
                                let win = window.clone();
                                let retries = config.resolution_poll_retries;
                                let initial_delay_ms = config.resolution_initial_delay_ms;
                                let delay_ms = config.resolution_poll_delay_ms;
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(
                                        initial_delay_ms,
                                    ))
                                    .await;
                                    if let Some(outcome) = crate::market_discovery::poll_resolution(
                                        &gamma_url, &slug, retries, delay_ms,
                                    ).await {
                                        let _ = tx.send(DeferredResolution {
                                            market_id: mid,
                                            window: win,
                                            authoritative_outcome: outcome,
                                        }).await;
                                    }
                                });
                            }

                            if state.current_window.as_ref().is_some_and(|w| w.market_id == closed_id) {
                                state.current_window = None;
                                state.signal_state.book_state = crate::types::BookState::default();
                            }
                        }
                    }
                }
            }

            window = activate_rx.recv() => {
                if let Some(window) = window {
                    activate_window(&mut state, &window, &clob_handle);
                }
            }

            _ = storage_report_timer.tick() => {
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

            resolution = resolution_rx.recv() => {
                if let Some(res) = resolution {
                    handle_deferred_resolution(
                        &res,
                        &db,
                        &mut position_manager,
                        &mut bankroll,
                        &mut trend_tracker,
                        &mut circuit_breaker,
                        &config,
                        &clock,
                    );
                }
            }

            _ = &mut shutdown_rx => {
                info!("shutdown signal received, shutting down");
                break;
            }
        }
    }

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

    if let Some(bp) = state.signal_state.binance_price {
        state
            .window_open_prices
            .entry(window.market_id.clone())
            .or_insert(bp);
    }

    state.current_window = Some(window.clone());
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

/// Process a deferred resolution from the background Gamma polling task.
/// Applies authoritative settlement exactly once for the closed window.
#[allow(clippy::too_many_arguments)]
fn handle_deferred_resolution(
    res: &DeferredResolution,
    db: &Database,
    position_manager: &mut PositionManager,
    bankroll: &mut BankrollManager,
    trend_tracker: &mut TrendTracker,
    circuit_breaker: &mut CircuitBreaker,
    config: &Config,
    clock: &dyn Clock,
) {
    let now = clock.now();
    let auth_str = res.authoritative_outcome.to_string();

    let resolved = position_manager.resolve_window_with_outcome(
        &res.window,
        res.authoritative_outcome,
        db,
        bankroll,
        config,
        clock,
    );

    for (trade, result) in &resolved {
        let won = result.pnl_net > 0.0;
        trend_tracker.record_outcome(trade.side, won, now);
        circuit_breaker.record_result(won, now);
        if let Some(trade_id) = trade.id {
            let prediction = trade.side.to_string();
            let _ = db.log_settlement_audit(trade_id, &res.market_id, &prediction, &auth_str, now);
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
    trend_tracker: &mut TrendTracker,
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

    for strategy in strategies.iter_mut() {
        let result = strategy.evaluate(&ctx, config, now);

        match result {
            StrategyResult::None => {}
            StrategyResult::Single(signal) => {
                if trend_tracker.should_suppress(signal.direction) {
                    if let Ok(signal_id) = db.log_signal_with_context(
                        &signal,
                        Some(&window.market_id),
                        Some(ReplayFidelity::RawEvent),
                        None,
                        None,
                    ) {
                        if let Some(telemetry) = signal.telemetry.as_ref() {
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
                    info!(
                        strategy = %signal.strategy,
                        direction = %signal.direction,
                        "signal suppressed by trend filter"
                    );
                    continue;
                }

                info!(
                    strategy = %signal.strategy,
                    direction = %signal.direction,
                    confidence = signal.confidence,
                    "signal generated"
                );

                if let Ok(Some(_)) = execution_engine.submit_single(
                    &signal,
                    window,
                    db,
                    bankroll,
                    config,
                    clock,
                    ReplayFidelity::RawEvent,
                ) {
                    tracing::debug!(strategy = %signal.strategy, "order queued");
                }
            }
            StrategyResult::Batch(signals) => {
                info!(
                    strategy = signals.first().map_or("?", |s| &s.strategy),
                    count = signals.len(),
                    "batch signal generated"
                );

                let _ = execution_engine.submit_spread(
                    &signals,
                    window,
                    db,
                    bankroll,
                    config,
                    clock,
                    ReplayFidelity::RawEvent,
                );
            }
        }
    }
}
