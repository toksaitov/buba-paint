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
use crate::market_discovery::{self, MarketDiscoveryEvent};
use crate::position_manager::PositionManager;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategies::{Strategy, StrategyResult};
use crate::tick_logger::{self, TickLoggerState};
use crate::trend_tracker::TrendTracker;
use crate::types::{
    BookState, FeedEvent, MarketWindow, ReplayFidelity, SignalDirection, StrategyContext,
};

struct DeferredResolution {
    market_id: String,
    window: MarketWindow,
    authoritative_outcome: SignalDirection,
}

struct LiveState {
    binance_price: Option<f64>,
    chainlink_price: Option<f64>,
    book_state: BookState,
    current_window: Option<MarketWindow>,
    /// Open prices captured per `market_id` (so we can settle any window,
    /// not just the current one).
    window_open_prices: std::collections::HashMap<String, f64>,
    /// All windows we've seen (for resolving trades when window closes).
    known_windows: std::collections::HashMap<String, MarketWindow>,
}

impl LiveState {
    /// Creates a new `LiveState`.
    fn new() -> Self {
        Self {
            binance_price: None,
            chainlink_price: None,
            book_state: BookState::default(),
            current_window: None,
            window_open_prices: std::collections::HashMap::new(),
            known_windows: std::collections::HashMap::new(),
        }
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

    let (resolution_tx, mut resolution_rx) = tokio::sync::mpsc::channel::<DeferredResolution>(32);

    let (activate_tx, mut activate_rx) = tokio::sync::mpsc::channel::<MarketWindow>(32);

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
                    FeedMessage::BinanceTick { price, timestamp, payload_json } => {
                        let received_at_ms = clock.now();
                        state.binance_price = Some(price);
                        momentum.push(price, timestamp);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.binance_price = Some(price);
                        }

                        let _ = db.log_feed_event(&FeedEvent {
                            id: None,
                            received_at_ms,
                            event_at_ms: timestamp,
                            source: "binance".to_string(),
                            event_type: "binance_tick".to_string(),
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: Some(price),
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            payload_json,
                            fidelity: ReplayFidelity::RawEvent,
                        });

                        if let Some(ref w) = state.current_window {
                            state.window_open_prices.entry(w.market_id.clone()).or_insert(price);
                        }

                        let _ = execution_engine.process_due_orders(
                            received_at_ms,
                            state.current_window.as_ref(),
                            &state.book_state,
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
                            received_at_ms,
                        );
                    }

                    FeedMessage::ChainlinkPrice { price, timestamp, payload_json } => {
                        let received_at_ms = clock.now();
                        state.chainlink_price = Some(price);
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.chainlink_price = Some(price);
                        }

                        let _ = db.log_feed_event(&FeedEvent {
                            id: None,
                            received_at_ms,
                            event_at_ms: timestamp,
                            source: "chainlink".to_string(),
                            event_type: "chainlink_price".to_string(),
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: Some(price),
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            payload_json,
                            fidelity: ReplayFidelity::RawEvent,
                        });

                        let _ = execution_engine.process_due_orders(
                            received_at_ms,
                            state.current_window.as_ref(),
                            &state.book_state,
                            &db,
                            &mut bankroll,
                            &config,
                            &clock,
                        );
                    }

                    FeedMessage::ClobBook { book_state, timestamp, payload_json } => {
                        let received_at_ms = clock.now();
                        state.book_state = book_state.clone();
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        let _ = log_live_clob_event(
                            &db,
                            received_at_ms,
                            timestamp,
                            "clob_snapshot",
                            &book_state,
                            state.current_window.as_ref(),
                            payload_json.as_deref(),
                        );

                        let _ = execution_engine.process_due_orders(
                            received_at_ms,
                            state.current_window.as_ref(),
                            &state.book_state,
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
                            received_at_ms,
                        );
                    }

                    FeedMessage::ClobPriceChange { book_state, timestamp, payload_json } => {
                        let received_at_ms = clock.now();
                        state.book_state = book_state.clone();
                        execution_engine.note_replay_fidelity(ReplayFidelity::RawEvent);

                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        let _ = log_live_clob_event(
                            &db,
                            received_at_ms,
                            timestamp,
                            "clob_price_change",
                            &book_state,
                            state.current_window.as_ref(),
                            payload_json.as_deref(),
                        );

                        let _ = execution_engine.process_due_orders(
                            received_at_ms,
                            state.current_window.as_ref(),
                            &state.book_state,
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
                            received_at_ms,
                        );
                    }

                    FeedMessage::FeedConnected(name) => {
                        info!(feed = %name, "feed connected");
                    }

                    FeedMessage::FeedDisconnected(name) => {
                        warn!(feed = %name, "feed disconnected");
                    }

                    FeedMessage::ChainlinkStale => {
                        warn!("chainlink price is stale");
                        state.chainlink_price = None;
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
                                state.binance_price.unwrap_or(0.0)
                            });
                            let close = state.binance_price.unwrap_or(open);
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
                                state.book_state = BookState::default();
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

    state.book_state = BookState::default();

    if let Some(bp) = state.binance_price {
        state
            .window_open_prices
            .entry(window.market_id.clone())
            .or_insert(bp);
    }

    state.current_window = Some(window.clone());
}

/// Logs live clob event.
fn log_live_clob_event(
    db: &Database,
    received_at_ms: u64,
    event_at_ms: u64,
    event_type: &str,
    book_state: &BookState,
    current_window: Option<&MarketWindow>,
    payload_json: Option<&str>,
) -> anyhow::Result<()> {
    for (source, asset_id, book) in [
        (
            "clob_up",
            current_window.map(|window| window.up_token_id.clone()),
            book_state.up.as_ref(),
        ),
        (
            "clob_down",
            current_window.map(|window| window.down_token_id.clone()),
            book_state.down.as_ref(),
        ),
    ] {
        if let Some(book) = book {
            db.log_feed_event(&FeedEvent {
                id: None,
                received_at_ms,
                event_at_ms,
                source: source.to_string(),
                event_type: event_type.to_string(),
                market_id: current_window.map(|window| window.market_id.clone()),
                asset_id,
                price: None,
                best_bid: Some(book.best_bid),
                best_ask: Some(book.best_ask),
                bid_size: Some(book.bid_size),
                ask_size: Some(book.ask_size),
                payload_json: payload_json.map(ToString::to_string),
                fidelity: ReplayFidelity::RawEvent,
            })?;
        }
    }

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
    let Some(binance_price) = state.binance_price else {
        return;
    };

    if !circuit_breaker.can_trade(now) {
        circuit_breaker.log_if_paused(now);
        return;
    }

    let ctx = StrategyContext {
        binance_price,
        binance_momentum: momentum.get(),
        chainlink_price: state.chainlink_price,
        book_state: state.book_state.clone(),
        window_time_remaining_ms: window.end_time.saturating_sub(now),
    };

    for strategy in strategies.iter_mut() {
        let result = strategy.evaluate(&ctx, config, now);

        match result {
            StrategyResult::None => {}
            StrategyResult::Single(signal) => {
                if trend_tracker.should_suppress(signal.direction) {
                    let _ = db.log_signal(&signal);
                    info!(
                        strategy = %signal.strategy,
                        direction = %signal.direction,
                        "signal suppressed by trend filter"
                    );
                    continue;
                }

                let _ = db.log_signal(&signal);
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
