// Live paper trading bot — extracted from main.rs so it can be driven
// by both the CLI and integration tests.

use std::sync::Arc;

use tracing::{error, info, warn};

use crate::backtest::momentum::MomentumCalculator;
use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::db::database::Database;
use crate::feeds::FeedMessage;
use crate::market_discovery::{self, MarketDiscoveryEvent};
use crate::position_manager::PositionManager;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategies::{Strategy, StrategyResult};
use crate::tick_logger::{self, TickLoggerState};
use crate::trend_tracker::TrendTracker;
use crate::types::{BookState, MarketWindow, StrategyContext};

// ---------------------------------------------------------------------------
// Live state — maintained in the main loop, updated from feed messages
// ---------------------------------------------------------------------------

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
    last_eval_time: u64,
}

impl LiveState {
    fn new() -> Self {
        Self {
            binance_price: None,
            chainlink_price: None,
            book_state: BookState::default(),
            current_window: None,
            window_open_prices: std::collections::HashMap::new(),
            known_windows: std::collections::HashMap::new(),
            last_eval_time: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

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

    // 1. Open database.
    let db = Database::new(db_path)?;

    // 2. Create core components.
    let clock = SystemClock;
    let mut bankroll = BankrollManager::new(balance, &config, &db, &clock);
    let mut position_manager = PositionManager::new();
    let mut circuit_breaker = CircuitBreaker::new(
        config.circuit_breaker_losses as u32,
        config.circuit_breaker_pause_ms,
    );
    let mut trend_tracker = TrendTracker::new(
        config.trend_filter_window as usize,
        config.trend_filter_enabled,
        config.trend_filter_threshold,
    );

    // 3. Create strategies.
    let mut strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(LatencyArbStrategy::new(
            config.latency_arb_momentum_threshold,
        )),
        Box::new(SpreadCaptureStrategy::new()),
    ];

    // Momentum calculator (reuse from backtest module).
    let mut momentum = MomentumCalculator::new(config.momentum_window_ms);

    // 4. Shared feed channel.
    let (feed_tx, mut feed_rx) = tokio::sync::mpsc::channel::<FeedMessage>(512);

    // 5. Spawn feed tasks.
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

    // 6. Spawn market discovery task.
    let mut discovery = market_discovery::run_market_discovery(&config).await;

    // 7. Spawn tick logger task.
    let tick_logger_state = Arc::new(tokio::sync::RwLock::new(TickLoggerState::default()));
    let tick_logger_state_clone = Arc::clone(&tick_logger_state);
    let tick_interval = config.tick_interval;
    let tick_logger_db_path = db_path.to_string();
    tokio::spawn(async move {
        tick_logger::run_tick_logger(tick_logger_db_path, tick_interval, tick_logger_state_clone)
            .await;
    });

    // 8. Live state.
    let mut state = LiveState::new();

    info!("all tasks spawned, entering main loop");

    // Pin the shutdown receiver so it can be polled repeatedly in select!.
    let mut shutdown_rx = shutdown_rx;

    // === Main loop ===
    loop {
        tokio::select! {
            // Feed messages.
            msg = feed_rx.recv() => {
                let Some(msg) = msg else {
                    warn!("all feed senders dropped, shutting down");
                    break;
                };

                match msg {
                    FeedMessage::BinanceTick { price, timestamp } => {
                        state.binance_price = Some(price);
                        momentum.push(price, timestamp);

                        // Update tick logger state.
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.binance_price = Some(price);
                        }

                        // Capture window open price from Binance if not set.
                        if let Some(ref w) = state.current_window {
                            state.window_open_prices.entry(w.market_id.clone()).or_insert(price);
                        }

                        // Run strategy evaluation (throttled).
                        evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut bankroll,
                            &mut position_manager,
                            &mut circuit_breaker,
                            &mut trend_tracker,
                        );
                    }

                    FeedMessage::ChainlinkPrice { price, timestamp: _ } => {
                        state.chainlink_price = Some(price);

                        // Update tick logger state.
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.chainlink_price = Some(price);
                        }
                    }

                    FeedMessage::ClobBook { book_state } | FeedMessage::ClobPriceChange { book_state } => {
                        state.book_state = book_state.clone();

                        // Update tick logger state.
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state;
                        }

                        // Run strategy evaluation (throttled).
                        evaluate_strategies(
                            &mut state,
                            &momentum,
                            &config,
                            &clock,
                            &db,
                            &mut strategies,
                            &mut bankroll,
                            &mut position_manager,
                            &mut circuit_breaker,
                            &mut trend_tracker,
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

            // Market discovery events.
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
                            "new market window"
                        );

                        // Upsert market in DB.
                        if let Err(e) = db.upsert_market(&window) {
                            error!("failed to upsert market: {e}");
                        }

                        // Tell CLOB feed to resubscribe to new token IDs.
                        let up_token = window.up_token_id.clone();
                        let down_token = window.down_token_id.clone();
                        let clob_handle_clone = clob_handle.clone();
                        tokio::spawn(async move {
                            if let Err(e) = clob_handle_clone.resubscribe(up_token, down_token).await {
                                error!("failed to resubscribe CLOB feed: {e}");
                            }
                        });

                        // Reset book state for new window.
                        state.book_state = BookState::default();

                        // Track open price and window for later settlement.
                        if let Some(bp) = state.binance_price {
                            state.window_open_prices.entry(window.market_id.clone()).or_insert(bp);
                        }
                        state.known_windows.insert(window.market_id.clone(), window.clone());
                        state.current_window = Some(window);
                    }

                    MarketDiscoveryEvent::WindowClosed(closed_window) => {
                        info!(market_id = %closed_window.market_id, "market window closed");

                        // Resolve positions for the closed window.  We look it
                        // up in `known_windows` (not `current_window`) because
                        // market discovery may have already advanced
                        // `current_window` to the next slot.
                        let closed_id = closed_window.market_id.clone();
                        if let Some(window) = state.known_windows.remove(&closed_id) {
                            let open_price = state.window_open_prices.remove(&closed_id).unwrap_or(0.0);
                            let close_price = state.binance_price.unwrap_or(open_price);

                            let resolved = position_manager.resolve_window(
                                &window,
                                open_price,
                                close_price,
                                &db,
                                &mut bankroll,
                                &config,
                                &clock,
                            );

                            let now = clock.now();
                            for (trade, result) in &resolved {
                                let won = result.pnl_0pct > 0.0;
                                trend_tracker.record_outcome(trade.side, won, now);
                                circuit_breaker.record_result(won, now);

                                let outcome = if won { "WIN" } else { "LOSS" };
                                info!(
                                    trade_id = trade.id.unwrap_or(-1),
                                    strategy = %trade.strategy,
                                    side = %trade.side,
                                    pnl = result.pnl_0pct,
                                    outcome,
                                    "trade settled"
                                );
                            }

                            let bankroll_stats = bankroll.get_stats();
                            info!(
                                balance = bankroll_stats.current_balance,
                                pnl = bankroll_stats.total_pnl,
                                trades = bankroll_stats.total_trades,
                                win_rate = format!("{:.1}%", bankroll_stats.win_rate * 100.0),
                                max_dd = format!("{:.1}%", bankroll_stats.max_drawdown_pct * 100.0),
                                "bankroll update"
                            );

                            // If this was the current window, clear it.
                            if state.current_window.as_ref().is_some_and(|w| w.market_id == closed_id) {
                                state.current_window = None;
                                state.book_state = BookState::default();
                            }
                        }
                    }
                }
            }

            // Graceful shutdown via oneshot receiver.
            _ = &mut shutdown_rx => {
                info!("shutdown signal received, shutting down");
                break;
            }
        }
    }

    // --- Graceful shutdown ---
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

// ---------------------------------------------------------------------------
// Strategy evaluation (200ms throttle)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn evaluate_strategies(
    state: &mut LiveState,
    momentum: &MomentumCalculator,
    config: &Config,
    clock: &SystemClock,
    db: &Database,
    strategies: &mut [Box<dyn Strategy>],
    bankroll: &mut BankrollManager,
    position_manager: &mut PositionManager,
    circuit_breaker: &mut CircuitBreaker,
    trend_tracker: &mut TrendTracker,
) {
    let now = clock.now();

    // 200ms throttle.
    if now.saturating_sub(state.last_eval_time) < 200 {
        return;
    }
    state.last_eval_time = now;

    // Need an active window and a Binance price.
    let Some(window) = state.current_window.as_ref() else {
        return;
    };
    let Some(binance_price) = state.binance_price else {
        return;
    };

    // Circuit breaker check.
    if !circuit_breaker.can_trade(now) {
        circuit_breaker.log_if_paused(now);
        return;
    }

    // Build strategy context.
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
                // Trend tracker suppression.
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

                if let Some(trade) =
                    position_manager.try_open(&signal, window, false, db, bankroll, config, clock)
                {
                    info!(
                        trade_id = trade.id.unwrap_or(-1),
                        strategy = %trade.strategy,
                        side = %trade.side,
                        entry_price = trade.entry_price,
                        size = trade.size,
                        "trade opened"
                    );
                }
            }
            StrategyResult::Batch(signals) => {
                for signal in &signals {
                    let _ = db.log_signal(signal);
                }
                info!(
                    strategy = signals.first().map_or("?", |s| &s.strategy),
                    count = signals.len(),
                    "batch signal generated"
                );

                let trades =
                    position_manager.try_open_spread(&signals, window, db, bankroll, config, clock);
                for trade in &trades {
                    info!(
                        trade_id = trade.id.unwrap_or(-1),
                        strategy = %trade.strategy,
                        side = %trade.side,
                        entry_price = trade.entry_price,
                        size = trade.size,
                        "spread trade opened"
                    );
                }
            }
        }
    }
}
