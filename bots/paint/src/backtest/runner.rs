use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, bail};

use crate::bankroll::BankrollManager;
use crate::circuit_breaker::CircuitBreaker;
use crate::clock::{BacktestClock, Clock};
use crate::config::{BacktestSettlementMode, Config};
use crate::db::database::Database;
use crate::executor::ExecutionEngine;
use crate::portfolio::{PortfolioAttributionStats, PortfolioRegime, StrategyFamily};
use crate::position_manager::PositionManager;
use crate::rejection_diagnostics::StrategyRejectionTracker;
use crate::signal_features::SignalFeatureEngine;
use crate::strategies::Strategy;
use crate::strategies::calm_persistence::CalmPersistenceStrategy;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::strategy_cycle::{family_for_strategy, run_strategy_cycle};
use crate::trend_tracker::ScopedTrendTracker;
use crate::types::{MarketWindow, SignalDirection, StrategyContext};

use super::feed_state::FeedState;
use super::momentum::MomentumCalculator;
use super::tick_replay::{SharedTicks, TickReplay};
use super::window_manager::WindowManager;

/// Summary metrics and attribution emitted by one completed backtest run.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
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
    pub gross_pnl: f64,
    pub max_drawdown_pct: f64,
    pub high_water_mark: f64,
    pub total_fees: f64,
    pub pnl_net: f64,
    pub fill_rate: f64,
    pub partial_fill_rate: f64,
    pub no_fill_count: u64,
    pub spread_legging_count: u64,
    pub residual_position_count: u64,
    pub avg_fill_latency_ms: f64,
    pub avg_slippage: f64,
    pub raw_event_batches: u64,
    pub legacy_snapshot_batches: u64,
    pub dislocation_regime_count: u64,
    pub structural_pair_regime_count: u64,
    pub calm_regime_count: u64,
    pub dislocation_queued: u64,
    pub structural_pair_queued: u64,
    pub calm_queued: u64,
    pub dislocation_filled: u64,
    pub structural_pair_filled: u64,
    pub calm_filled: u64,
    pub dislocation_missed: u64,
    pub structural_pair_missed: u64,
    pub calm_missed: u64,
    pub latency_arb_candidates: u64,
    pub spread_capture_candidates: u64,
    pub calm_persistence_candidates: u64,
    pub router_blocked_count: u64,
    pub capital_blocked_count: u64,
    pub latency_spread_overlap_count: u64,
    pub latency_calm_overlap_count: u64,
    pub spread_calm_overlap_count: u64,
}

/// How tick data is sourced.
#[allow(missing_docs)]
pub enum TickSource {
    /// Load from a database file.
    FromDb(String),
    /// Use pre-loaded ticks (shared across parallel sweep runs).
    Cached(SharedTicks),
}

/// Inputs controlling one backtest run.
#[allow(missing_docs)]
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

/// Build the enabled strategy list for one backtest configuration.
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

struct PendingBacktestSettlement {
    window: MarketWindow,
    outcome: SignalDirection,
}

struct ObservedResolutionSchedule {
    market_resolution_at_ms: HashMap<String, u64>,
    default_delay_ms: u64,
}

/// Load observed market-resolution timestamps from the exact pulled live run.
fn load_observed_resolution_schedule(
    conn: &rusqlite::Connection,
    config: &Config,
) -> anyhow::Result<ObservedResolutionSchedule> {
    let mut stmt = conn.prepare_cached(
        "SELECT st.market_id, m.end_time, MIN(tr.resolved_at) AS resolved_at
         FROM simulated_trades st
         JOIN markets m
           ON m.market_id = st.market_id
         JOIN trade_results tr
           ON tr.trade_id = st.id
         GROUP BY st.market_id, m.end_time",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u64>(1)?,
            row.get::<_, u64>(2)?,
        ))
    })?;

    let mut market_resolution_at_ms = HashMap::new();
    let mut delays = Vec::new();
    for row in rows {
        let (market_id, end_time, resolved_at) = row?;
        market_resolution_at_ms.insert(market_id, resolved_at);
        delays.push(resolved_at.saturating_sub(end_time));
    }
    delays.sort_unstable();
    let default_delay_ms = delays
        .get(delays.len() / 2)
        .copied()
        .unwrap_or(config.resolution_initial_delay_ms);

    Ok(ObservedResolutionSchedule {
        market_resolution_at_ms,
        default_delay_ms,
    })
}

/// Persist one market-close lifecycle update and reclassify open trades into pending settlement.
fn close_market_and_reclassify_reserve(
    db: &Database,
    bankroll: &mut BankrollManager,
    window: &MarketWindow,
) -> anyhow::Result<()> {
    db.resolve_market(&window.market_id, "closed")?;
    for trade in db.get_open_trades_for_market(&window.market_id)? {
        bankroll.transition_trade_to_pending_settlement(
            trade.entry_price * trade.size,
            &trade.strategy,
        );
    }
    Ok(())
}

/// Persist one batch of drained rejection summaries into the results database.
fn persist_rejection_summaries(
    db: &Database,
    tracker: &mut StrategyRejectionTracker,
    market_id: Option<&str>,
    timestamp_ms: u64,
) -> anyhow::Result<()> {
    let rows = if let Some(market_id) = market_id {
        tracker.drain_market(market_id, timestamp_ms)
    } else {
        tracker.drain_all(timestamp_ms)
    };
    for row in rows {
        let _ = db.log_strategy_rejection_summary(&row)?;
    }
    Ok(())
}

/// Move one market into the pending-settlement queue for later authoritative settlement.
fn schedule_pending_market_settlement(
    pending_settlements: &mut BTreeMap<u64, Vec<PendingBacktestSettlement>>,
    settle_at_ms: u64,
    window: &MarketWindow,
    outcome: SignalDirection,
) {
    pending_settlements
        .entry(settle_at_ms)
        .or_default()
        .push(PendingBacktestSettlement {
            window: window.clone(),
            outcome,
        });
}

/// Settle every pending market whose observed settlement time is now due.
#[allow(clippy::too_many_arguments)]
fn process_due_market_settlements(
    pending_settlements: &mut BTreeMap<u64, Vec<PendingBacktestSettlement>>,
    now_ms: u64,
    clock: &BacktestClock,
    results_db: &Database,
    position_manager: &mut PositionManager,
    bankroll: &mut BankrollManager,
    trend_tracker: &mut ScopedTrendTracker,
    circuit_breaker: &mut CircuitBreaker,
    config: &Config,
) {
    let due_times = pending_settlements
        .keys()
        .copied()
        .take_while(|timestamp| *timestamp <= now_ms)
        .collect::<Vec<_>>();

    for due_time in due_times {
        clock.set(due_time);
        if let Some(settlements) = pending_settlements.remove(&due_time) {
            for settlement in settlements {
                let resolved = position_manager.resolve_window_with_outcome(
                    &settlement.window,
                    settlement.outcome,
                    results_db,
                    bankroll,
                    config,
                    clock,
                );
                for (trade, result) in &resolved {
                    let won = result.pnl_net > 0.0;
                    trend_tracker.record_outcome(
                        family_for_strategy(&trade.strategy),
                        trade.side,
                        won,
                        due_time,
                    );
                    circuit_breaker.record_result(won, due_time);
                }
            }
        }
    }
}

/// Run one backtest over the provided historical interval and return summary metrics.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn run_backtest(options: BacktestOptions) -> anyhow::Result<BacktestResult> {
    let t0 = Instant::now();
    options.config.validate()?;

    let mut tick_replay = match &options.tick_source {
        TickSource::FromDb(path) => {
            let conn = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .with_context(|| format!("opening data DB: {path}"))?;
            TickReplay::from_db(&conn, options.start_time, options.end_time)?
        }
        TickSource::Cached(shared_ticks) => TickReplay::from_cached(Arc::clone(shared_ticks)),
    };

    for suffix in ["", "-shm", "-wal"] {
        let f = format!("{}{suffix}", options.results_db_path);
        let _ = std::fs::remove_file(&f);
    }

    let results_db = Database::new(&options.results_db_path)?;

    let clock = BacktestClock::new();

    let config = &options.config;
    let pending_policy = config.pending_settlement_policy_unchecked();
    let mut bankroll = BankrollManager::new(options.starting_balance, config, &results_db, &clock);
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

    let mut strategies = build_strategies(config);
    let enabled_strategies = config.enabled_strategy_names();
    if strategies.is_empty() {
        bail!(
            "no strategies enabled after config parsing; boolean values must be true/false or 1/0"
        );
    }
    let mut rejection_tracker = StrategyRejectionTracker::new();
    let mut pending_settlements = BTreeMap::new();

    let mut feed_state = FeedState::new();
    let mut momentum = MomentumCalculator::new(config.momentum_window_ms);

    let data_conn = rusqlite::Connection::open_with_flags(
        &options.data_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("opening data DB for windows: {}", options.data_db_path))?;
    let mut window_manager = WindowManager::new(&data_conn, options.start_time, options.end_time)?;
    let observed_resolution_schedule = match config.backtest_settlement_mode {
        BacktestSettlementMode::Immediate => None,
        BacktestSettlementMode::ObservedMarketResolution => {
            Some(load_observed_resolution_schedule(&data_conn, config)?)
        }
    };

    let total_ticks = tick_replay.total_ticks();
    let total_windows = window_manager.total_windows();

    if !options.quiet {
        let duration_h = (options.end_time - options.start_time) as f64 / 3_600_000.0;
        println!(
            "Backtesting {duration_h:.1}h | {total_ticks} ticks | {total_windows} windows | balance=${} | strategies={} | settlement_mode={} | pending_mode={} ({:.2}/{:.2}/{})",
            options.starting_balance,
            enabled_strategies.join(","),
            config.backtest_settlement_mode.as_str(),
            pending_policy.mode.as_str(),
            pending_policy.family_reserve_fraction,
            pending_policy.global_reserve_fraction,
            pending_policy.counts_as_open_position,
        );
    }

    let mut signal_count: u64 = 0;
    let mut portfolio_stats = PortfolioAttributionStats::default();

    while let Some(group) = tick_replay.next_group() {
        let replay_ts = group.timestamp;
        clock.set(replay_ts);
        process_due_market_settlements(
            &mut pending_settlements,
            replay_ts,
            &clock,
            &results_db,
            &mut position_manager,
            &mut bankroll,
            &mut trend_tracker,
            &mut circuit_breaker,
            config,
        );
        clock.set(replay_ts);
        execution_engine.note_replay_fidelity(group.fidelity);

        feed_state.update(&group);

        let current_window_for_execution = window_manager
            .current
            .as_ref()
            .map(WindowManager::to_market_window);

        let opened_now = execution_engine.process_due_orders(
            replay_ts,
            None,
            current_window_for_execution.as_ref(),
            &feed_state.signal_state.book_state,
            &results_db,
            &mut bankroll,
            config,
            &clock,
        )?;

        for trade in &opened_now {
            if let Some(id) = trade.id {
                tracing::debug!(
                    trade_id = id,
                    strategy = %trade.strategy,
                    side = %trade.side,
                    fill_status = trade.fill_status.as_deref().unwrap_or("unknown"),
                    "simulated order filled"
                );
            }
        }
        let processed_outcomes = execution_engine.take_recent_outcomes();
        portfolio_stats.record_processed_outcomes(&processed_outcomes);

        if let Some(ref binance) = group.binance {
            if let Some(price) = binance.price {
                momentum.push(price, group.timestamp);
            }
        }

        let events = window_manager.advance(group.timestamp);

        if let Some(ref closed) = events.closed {
            let mw = WindowManager::to_market_window(closed);

            let _ = results_db.upsert_market(&mw);
            close_market_and_reclassify_reserve(&results_db, &mut bankroll, &mw)?;
            persist_rejection_summaries(
                &results_db,
                &mut rejection_tracker,
                Some(&mw.market_id),
                replay_ts,
            )?;
            match config.backtest_settlement_mode {
                BacktestSettlementMode::Immediate => {
                    let resolved = position_manager.resolve_window_with_outcome(
                        &mw,
                        closed.outcome,
                        &results_db,
                        &mut bankroll,
                        config,
                        &clock,
                    );

                    for (trade, result) in &resolved {
                        let won = result.pnl_net > 0.0;
                        trend_tracker.record_outcome(
                            family_for_strategy(&trade.strategy),
                            trade.side,
                            won,
                            replay_ts,
                        );
                        circuit_breaker.record_result(won, replay_ts);
                    }
                }
                BacktestSettlementMode::ObservedMarketResolution => {
                    let settle_at_ms = observed_resolution_schedule
                        .as_ref()
                        .and_then(|schedule| {
                            schedule
                                .market_resolution_at_ms
                                .get(&mw.market_id)
                                .copied()
                                .or_else(|| {
                                    Some(
                                        mw.end_time.saturating_add(
                                            schedule
                                                .default_delay_ms
                                                .max(config.resolution_initial_delay_ms),
                                        ),
                                    )
                                })
                        })
                        .unwrap_or_else(|| {
                            mw.end_time
                                .saturating_add(config.resolution_initial_delay_ms)
                        })
                        .max(replay_ts);
                    schedule_pending_market_settlement(
                        &mut pending_settlements,
                        settle_at_ms,
                        &mw,
                        closed.outcome,
                    );
                }
            }
        }

        if let Some(ref opened) = events.opened {
            let mw = WindowManager::to_market_window(opened);
            let _ = results_db.upsert_market(&mw);

            feed_state.signal_state.book_state = crate::types::BookState::default();
        }

        let Some(current_window) = window_manager.current.as_ref() else {
            continue;
        };
        let Some(binance_price) = feed_state.signal_state.binance_price else {
            continue;
        };
        let current_mw = WindowManager::to_market_window(current_window);
        let features = SignalFeatureEngine::compute(
            &mut feed_state.signal_state,
            Some(&current_mw),
            Some(current_window.open_price),
            momentum.get(),
            group.timestamp,
            group.timestamp_us,
            config,
        );

        let ctx = StrategyContext {
            binance_price,
            binance_momentum: momentum.get(),
            chainlink_price: feed_state.signal_state.chainlink_price,
            book_state: feed_state.signal_state.book_state.clone(),
            window_open_price: Some(current_window.open_price),
            window_time_remaining_ms: current_window.end_time.saturating_sub(group.timestamp),
            now_us: group.timestamp_us,
            features,
        };

        if !circuit_breaker.can_trade(replay_ts) {
            continue;
        }

        let cycle = run_strategy_cycle(
            &ctx,
            &current_mw,
            config,
            &clock,
            &results_db,
            &mut strategies,
            &mut execution_engine,
            &mut bankroll,
            &mut trend_tracker,
            &mut rejection_tracker,
            group.fidelity,
            replay_ts,
        )?;
        portfolio_stats.record_observed_regime(cycle.observed_regime);
        portfolio_stats.record_candidates(&cycle.candidate_families);
        for _ in 0..cycle.router_blocked_count {
            portfolio_stats.record_router_block();
        }
        signal_count += cycle.signal_count_delta;
        for submission in &cycle.submissions {
            portfolio_stats.record_submission(
                submission.family,
                submission.regime,
                &submission.outcome,
            );
        }
    }

    let pending_times = pending_settlements.keys().copied().collect::<Vec<_>>();
    for pending_time in pending_times {
        process_due_market_settlements(
            &mut pending_settlements,
            pending_time,
            &clock,
            &results_db,
            &mut position_manager,
            &mut bankroll,
            &mut trend_tracker,
            &mut circuit_breaker,
            config,
        );
    }
    persist_rejection_summaries(&results_db, &mut rejection_tracker, None, clock.now())?;

    let stats = bankroll.get_stats();
    let execution_stats = execution_engine.stats().clone();
    let elapsed = t0.elapsed().as_secs_f64();
    let gross_pnl = stats.total_pnl + stats.total_fees;
    let partial_fill_rate = if execution_stats.filled_orders == 0 {
        0.0
    } else {
        execution_stats.partial_fills as f64 / execution_stats.filled_orders as f64
    };

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
        gross_pnl,
        max_drawdown_pct: stats.max_drawdown_pct,
        high_water_mark: stats.high_water_mark,
        total_fees: stats.total_fees,
        pnl_net: stats.total_pnl,
        fill_rate: execution_stats.fill_rate(),
        partial_fill_rate,
        no_fill_count: execution_stats.no_fills,
        spread_legging_count: execution_stats.spread_legging_failures,
        residual_position_count: execution_stats.residual_positions,
        avg_fill_latency_ms: execution_stats.avg_fill_latency_ms().unwrap_or(0.0),
        avg_slippage: execution_stats.avg_slippage().unwrap_or(0.0),
        raw_event_batches: execution_stats.raw_event_batches,
        legacy_snapshot_batches: execution_stats.legacy_snapshot_batches,
        dislocation_regime_count: portfolio_stats.regime_count(PortfolioRegime::Dislocation),
        structural_pair_regime_count: portfolio_stats.regime_count(PortfolioRegime::StructuralPair),
        calm_regime_count: portfolio_stats.regime_count(PortfolioRegime::Calm),
        dislocation_queued: portfolio_stats.queued_count(PortfolioRegime::Dislocation),
        structural_pair_queued: portfolio_stats.queued_count(PortfolioRegime::StructuralPair),
        calm_queued: portfolio_stats.queued_count(PortfolioRegime::Calm),
        dislocation_filled: portfolio_stats.filled_count(PortfolioRegime::Dislocation),
        structural_pair_filled: portfolio_stats.filled_count(PortfolioRegime::StructuralPair),
        calm_filled: portfolio_stats.filled_count(PortfolioRegime::Calm),
        dislocation_missed: portfolio_stats.missed_count(PortfolioRegime::Dislocation),
        structural_pair_missed: portfolio_stats.missed_count(PortfolioRegime::StructuralPair),
        calm_missed: portfolio_stats.missed_count(PortfolioRegime::Calm),
        latency_arb_candidates: portfolio_stats.candidate_count(StrategyFamily::LatencyArb),
        spread_capture_candidates: portfolio_stats.candidate_count(StrategyFamily::SpreadCapture),
        calm_persistence_candidates: portfolio_stats
            .candidate_count(StrategyFamily::CalmPersistence),
        router_blocked_count: portfolio_stats.router_blocked_count,
        capital_blocked_count: portfolio_stats.capital_blocked_count,
        latency_spread_overlap_count: portfolio_stats.latency_spread_overlap_count,
        latency_calm_overlap_count: portfolio_stats.latency_calm_overlap_count,
        spread_calm_overlap_count: portfolio_stats.spread_calm_overlap_count,
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

    results_db.close();

    Ok(result)
}
