# Data and Replay

This document describes durable data, replay-grade capture, and backtesting constraints.

## Data Ownership

`runs/` contains primary run data from live paper and readonly sessions. Treat these DBs, logs, and analysis artifacts as irreplaceable. Do not edit files under `runs/` manually. The only supported in-place mutation is the additive `upgrade-history` workflow for historical run DBs.

`data/` contains derived data: sweeps, experiments, backfill cache, merged DBs, and reports. These are reproducible from `runs/` in principle, but many are still valuable and should not be deleted casually.

Database files must not be committed to Git or Git LFS history. Keep scratch DBs under `/tmp` or an explicit ignored data path.

## Replay-Grade Capture

New research runs should use:

```bash
FEED_EVENT_STORAGE_PROFILE=replay_grade
```

Replay-grade capture persists typed decision inputs needed for future sweeps:

- Binance `aggTrade` with price, size, signed quantity, event time, and receive time.
- Binance `bookTicker` with best bid, best ask, bid size, ask size, event time, and receive time.
- Binance `depth` with depth features.
- Chainlink price context.
- CLOB UP and DOWN top-of-book state with price, size, source timestamp where available, and local receive timestamp.
- Market metadata at discovery and activation.

`compact` is descriptive-only because it suppresses Binance book-ticker persistence. `full_debug` preserves payloads for short diagnostics and should not be a week-long run default.

## Replay Quality Gate

Run this before any long sweep:

```bash
cargo run -p buba-paint --release -- validate-replay-data \
  --data <db> \
  --start <time> \
  --end <time>
```

`buba-paint sweep` refuses inputs that are not `sweep_grade`. Backtests remain runnable on lower-fidelity archives, but those results must be labeled honestly.

Old runs that lack Binance book-ticker rows are descriptive evidence only. They can support postmortems, drawdown analysis, and operational diagnostics, but not trusted parameter selection.

## Database Tables

Important run DB tables:

- `run_metadata`: feed storage profile, replay-quality class, and observed feed-event classes.
- `feed_events`: canonical replay source when available.
- `tick_data`: 1-second sampled telemetry for dashboards and coarse inspection.
- `markets`: one row per 5-minute window with token IDs, status, resolution, fee profile, min size, tick size, rewards, and accepting-orders metadata.
- `signals`: strategy detection events.
- `signal_metrics`: signal feature snapshots, queue decisions, execution timing, fill/miss state, and rejection reasons.
- `strategy_rejection_summaries`: aggregated no-signal diagnostics.
- `feed_health_events`: connect, disconnect, stale, reconnect, and resubscribe telemetry.
- `simulated_trades`: opened paper positions.
- `trade_results`: authoritative settlement and PnL.
- `balance_log`: balance events and equity curve.
- live tables: live sessions, account snapshots, venue orders, fills, redemptions, reconciliation events, and control audit actions.

See `bots/paint/src/db/schema.rs` for full DDL. Schema evolution is additive through `add_column_if_missing`.

## Backtesting Model

When `feed_events` exist, the backtester replays raw events at recorded timestamps. If microsecond receive fields exist, replay orders by `received_at_us`; otherwise it falls back to millisecond ordering.

When `feed_events` are absent, the backtester synthesizes `legacy_snapshot` replay from `tick_data`. This path is lower fidelity and should not be described as true latency reconstruction.

Backtests use the shared strategy code, shared strategy cycle, shared fee model, and shared paper execution engine. The simulator models order-arrival latency, partial fills, no-fills, min-size checks, tick-size checks, liquidity constraints, and spread legging risk. It cannot reconstruct queue position or sub-second book changes that were never recorded.

## Settlement and Reserve Timing

Paper settlement is applied only on authoritative Polymarket outcomes. The bot may record provisional estimates for observability, but bankroll, Kelly state, trend tracking, and circuit breakers update only after authoritative settlement.

For exact pulled-run calibration:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution
```

This keeps trades in pending settlement until the observed authoritative resolution timestamp from the live run. Conservative pending-settlement reserve mode is the current baseline. See [pending-settlement-modes.md](./pending-settlement-modes.md).

## Useful SQL

```sql
SELECT 'feed_events' t, COUNT(*) FROM feed_events
UNION SELECT 'signals', COUNT(*) FROM signals
UNION SELECT 'signal_metrics', COUNT(*) FROM signal_metrics
UNION SELECT 'rejections', COUNT(*) FROM strategy_rejection_summaries
UNION SELECT 'trades', COUNT(*) FROM simulated_trades
UNION SELECT 'results', COUNT(*) FROM trade_results
UNION SELECT 'balance', COUNT(*) FROM balance_log;

SELECT timestamp_ms, market_id, strategy, reason, count, details_json
FROM strategy_rejection_summaries
ORDER BY timestamp_ms DESC
LIMIT 20;

SELECT t.strategy, COUNT(*) trades,
  SUM(CASE WHEN r.pnl_0pct > 0 THEN 1 ELSE 0 END) wins,
  ROUND(SUM(r.pnl_0pct), 2) total_pnl
FROM trade_results r
JOIN simulated_trades t ON r.trade_id = t.id
GROUP BY t.strategy;

SELECT signal_id, decision_status, rejection_reason,
       order_submitted_at_ms, expected_arrival_at_ms,
       order_processed_at_ms, effective_arrival_delay_ms
FROM signal_metrics
ORDER BY signal_id DESC
LIMIT 20;
```

## Run History

Historical run quality notes live in [runs.md](./runs.md). Run-specific postmortems and experiments belong under `data/experiments/...`, not in `docs/`.
