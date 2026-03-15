// Backtest runner — replays historical ticks through real strategy code.
//
// Direct port of the TypeScript `runBacktest` function.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;

use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::BacktestClock;
use crate::config::Config;
use crate::db::database::Database;
use crate::position_manager::PositionManager;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategies::{Strategy, StrategyResult};
use crate::trend_tracker::TrendTracker;
use crate::types::StrategyContext;

use super::feed_state::FeedState;
use super::momentum::MomentumCalculator;
use super::tick_replay::{RawTick, TickReplay};
use super::window_manager::WindowManager;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub start_time: u64,
    pub end_time: u64,
    pub duration_hours: f64,
    pub elapsed_seconds: f64,
    pub total_ticks: usize,
    pub total_windows: usize,
    pub signals: u64,
    pub trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub win_rate: f64,
    pub final_balance: f64,
    pub total_pnl: f64,
    pub max_drawdown_pct: f64,
    pub high_water_mark: f64,
}

/// How tick data is sourced.
pub enum TickSource {
    /// Load from a database file.
    FromDb(String),
    /// Use pre-loaded ticks (shared across parallel sweep runs).
    Cached(Arc<Vec<RawTick>>),
}

pub struct BacktestOptions {
    pub tick_source: TickSource,
    pub data_db_path: String,
    pub results_db_path: String,
    pub start_time: u64,
    pub end_time: u64,
    pub starting_balance: f64,
    pub quiet: bool,
    pub config: Config,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn run_backtest(options: BacktestOptions) -> anyhow::Result<BacktestResult> {
    let t0 = Instant::now();

    // Load or reference ticks.
    let ticks: Vec<RawTick> = match options.tick_source {
        TickSource::FromDb(ref path) => {
            let conn = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .with_context(|| format!("opening data DB: {path}"))?;
            TickReplay::load_ticks(&conn, options.start_time, options.end_time)?
        }
        TickSource::Cached(ref arc) => arc.as_ref().clone(),
    };

    // Delete any stale results DB to prevent the BankrollManager from
    // recovering a previous run's balance (which would corrupt sizing).
    for suffix in ["", "-shm", "-wal"] {
        let f = format!("{}{suffix}", options.results_db_path);
        let _ = std::fs::remove_file(&f);
    }

    // Create results DB (writable).
    let results_db = Database::new(&options.results_db_path)?;

    // Backtest clock.
    let clock = BacktestClock::new();

    // Initialize components.
    let config = &options.config;
    let mut bankroll = BankrollManager::new(options.starting_balance, config, &results_db, &clock);
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

    let mut strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(LatencyArbStrategy::new(
            config.latency_arb_momentum_threshold,
        )),
        Box::new(SpreadCaptureStrategy::new()),
    ];

    // Backtesting infrastructure.
    let mut tick_replay = TickReplay::from_cached(ticks);
    let mut feed_state = FeedState::new();
    let mut momentum = MomentumCalculator::new(config.momentum_window_ms);

    // Window manager loads from the data DB (read-only).
    let data_conn = rusqlite::Connection::open_with_flags(
        &options.data_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("opening data DB for windows: {}", options.data_db_path))?;
    let mut window_manager = WindowManager::new(&data_conn, options.start_time, options.end_time)?;

    let total_ticks = tick_replay.total_ticks();
    let total_windows = window_manager.total_windows();

    if !options.quiet {
        let duration_h = (options.end_time - options.start_time) as f64 / 3_600_000.0;
        println!(
            "Backtesting {duration_h:.1}h | {total_ticks} ticks | {total_windows} windows | balance=${}",
            options.starting_balance,
        );
    }

    let mut signal_count: u64 = 0;

    // === Main replay loop ===
    while let Some(group) = tick_replay.next_group() {
        let replay_ts = group.timestamp;
        clock.set(replay_ts);

        // Update feed state.
        feed_state.update(&group);

        // Push Binance price into momentum calculator.
        if let Some(ref binance) = group.binance {
            if let Some(price) = binance.price {
                momentum.push(price, group.timestamp);
            }
        }

        // Advance market windows.
        let events = window_manager.advance(group.timestamp);

        // Handle closed window.
        if let Some(ref closed) = events.closed {
            let mw = WindowManager::to_market_window(closed);
            // Upsert market in results DB so PositionManager can find it.
            let _ = results_db.upsert_market(&mw);
            // Resolve window — returns (trade, result) pairs.
            let resolved = position_manager.resolve_window(
                &mw,
                closed.open_price,
                closed.close_price,
                &results_db,
                &mut bankroll,
                config,
                &clock,
            );
            // Feed results to trend tracker and circuit breaker.
            for (trade, result) in &resolved {
                let won = result.pnl_0pct > 0.0;
                trend_tracker.record_outcome(trade.side, won, replay_ts);
                circuit_breaker.record_result(won, replay_ts);
            }
        }

        // Handle opened window.
        if let Some(ref opened) = events.opened {
            let mw = WindowManager::to_market_window(opened);
            let _ = results_db.upsert_market(&mw);
            // Clear CLOB book state — mirrors live bot's resubscribe().
            feed_state.book_state = crate::types::BookState::default();
        }

        // Skip evaluation if no active window or no Binance price.
        let Some(current_window) = window_manager.current.as_ref() else {
            continue;
        };
        let Some(binance_price) = feed_state.binance_price else {
            continue;
        };

        // Build strategy context.
        let ctx = StrategyContext {
            binance_price,
            binance_momentum: momentum.get(),
            chainlink_price: feed_state.chainlink_price,
            book_state: feed_state.book_state.clone(),
            window_time_remaining_ms: current_window.end_time.saturating_sub(group.timestamp),
        };

        // Circuit breaker.
        if !circuit_breaker.can_trade(replay_ts) {
            continue;
        }

        // Evaluate strategies.
        let current_mw = WindowManager::to_market_window(current_window);
        for strategy in &mut strategies {
            let result = strategy.evaluate(&ctx, config, replay_ts);

            match result {
                StrategyResult::None => {}
                StrategyResult::Single(signal) => {
                    // Check trend tracker suppression.
                    if trend_tracker.should_suppress(signal.direction) {
                        let _ = results_db.log_signal(&signal);
                        signal_count += 1;
                        continue;
                    }
                    let _ = results_db.log_signal(&signal);
                    signal_count += 1;
                    position_manager.try_open(
                        &signal,
                        &current_mw,
                        false,
                        &results_db,
                        &mut bankroll,
                        config,
                        &clock,
                    );
                }
                StrategyResult::Batch(signals) => {
                    for signal in &signals {
                        let _ = results_db.log_signal(signal);
                        signal_count += 1;
                    }
                    position_manager.try_open_spread(
                        &signals,
                        &current_mw,
                        &results_db,
                        &mut bankroll,
                        config,
                        &clock,
                    );
                }
            }
        }
    }

    // Gather results.
    let stats = bankroll.get_stats();
    let elapsed = t0.elapsed().as_secs_f64();

    let result = BacktestResult {
        start_time: options.start_time,
        end_time: options.end_time,
        duration_hours: (options.end_time - options.start_time) as f64 / 3_600_000.0,
        elapsed_seconds: elapsed,
        total_ticks,
        total_windows,
        signals: signal_count,
        trades: stats.total_trades,
        wins: stats.wins,
        losses: stats.losses,
        win_rate: stats.win_rate,
        final_balance: stats.current_balance,
        total_pnl: stats.total_pnl,
        max_drawdown_pct: stats.max_drawdown_pct,
        high_water_mark: stats.high_water_mark,
    };

    if !options.quiet {
        println!(
            "PnL=${:.2} | WR={:.1}% | Trades={} ({}W/{}L) | MaxDD={:.1}% | \
             Peak=${:.2} | {:.1}h replayed in {:.1}s",
            result.total_pnl,
            result.win_rate * 100.0,
            result.trades,
            result.wins,
            result.losses,
            result.max_drawdown_pct * 100.0,
            result.high_water_mark,
            result.duration_hours,
            result.elapsed_seconds,
        );
    }

    // Cleanup.
    results_db.close();

    Ok(result)
}
