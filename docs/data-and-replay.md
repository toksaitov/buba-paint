# Data And Replay

This chapter explains what the system records, what can be trusted for research, and how runtime SQLite data becomes a sweep input.

## Purpose

Buba is not useful if it only shows a chart after the fact. The run DB must preserve enough decision inputs to explain why a strategy accepted or rejected a trade, replay the same market state later, and compare parameter sets without inventing data that the bot never saw.

At the same time, the bot cannot sacrifice trading latency to do offline analysis. Runtime capture is append-oriented and bounded. Heavy validation, indexing, integrity checks, and parameter sweeps happen offline.

## Data Classes

Use these terms precisely:

* `replay_grade`: a configured storage profile that can capture the public decision inputs needed for close replay.
* `sweep_grade`: an observed interval that actually contains the required public feed classes.
* `backtest_ready`: an interval the current backtester can load, dry-run, and explain from the DB.
* `prepared_backtest`: a derived DB copied from a runtime DB, validated, and indexed for large sweeps.
* `research_grade_live`: a funded live interval with complete private decision, order, account, reconciliation, and control evidence.

`sweep_grade` is not enough for sweeps. Sweeps require `backtest_ready`. Funded live intervals also require `research_grade_live`.

## Runtime Storage Profiles

`FEED_EVENT_STORAGE_PROFILE=replay_grade` is the research default. It captures:

* Binance `aggTrade` price, size, signed quantity, event time, and receive time.
* Binance `bookTicker` best bid/ask and size.
* Binance depth features.
* Chainlink BTC/USD context.
* CLOB UP and DOWN top-of-book mutations with price, size, receive time, source identity, and market/token identity.
* Market metadata and fee/tick/min-size context.

`compact` is descriptive-only. It suppresses inputs needed for close replay and must not be described as sweep-ready.

`full_debug` preserves bulkier raw payloads for short investigations. It should not be the default for long runs.

## SQLite Layout

The operational database is SQLite. It is run-local, easy to copy, and reliable when used as an append-oriented capture store with bounded workers.

Important tables:

* `markets`: 5-minute window metadata, token IDs, status, outcome, fees, min size, tick size, and accepting-order metadata.
* `feed_events`: generic replay rows for Binance, Chainlink, metadata, and historical compatibility.
* `clob_replay_events`: legacy compact row storage for CLOB top-of-book data.
* `clob_replay_blocks`: default storage for new replay-grade CLOB top-of-book data. Blocks are versioned zstd-compressed payloads with min/max receive times, row counts, compressed/uncompressed byte counts, checksum, and typed event payload.
* `signals`: detected strategy signals.
* `signal_metrics`: feature snapshots, decision status, timing, rejection reason, and decision evidence.
* `strategy_rejection_summaries`: aggregated no-trade diagnostics.
* `simulated_trades`, `trade_results`, and `balance_log`: paper execution and equity history.
* live tables: sessions, account snapshots, order intents, venue orders, fills, redemptions, reconciliation events, control state, and control commands.
* `run_metadata`: runtime config snapshot, cheap runtime capture health, and offline validation results.

Full DDL lives in `bots/paint/src/db/schema.rs`.

## CLOB Replay Blocks

CLOB top-of-book updates dominate DB size in long readonly runs. New replay-grade DBs store CLOB top-of-book events in `clob_replay_blocks` instead of millions of wide text-heavy generic rows.

The block payload keeps the replay-critical fields:

* receive and event timestamps, including microsecond fields when available
* side and event type
* market and token identity
* source topic, connection, and sequence identity where available
* best bid, best ask, bid size, ask size, and microprice
* replay fidelity label

Size-only top-of-book mutations are retained. They affect liquidity, microprice, quote churn, fillability, and future strategy research.

Replay readers merge all supported shapes: `feed_events`, legacy `clob_replay_events`, and `clob_replay_blocks`. Existing historical DBs remain readable.

## Validation Sequence

Raw public replay completeness:

```bash
cargo run -p buba-paint --release -- validate-replay-data \
  --data <db> \
  --start <time> \
  --end <time>
```

Backtest tool compatibility:

```bash
cargo run -p buba-paint --release -- validate-backtest-input \
  --data <db> \
  --start <time> \
  --end <time>
```

Prepared sweep input:

```bash
cargo run -p buba-paint --release -- prepare-backtest-input \
  --data <runtime-db> \
  --start <time> \
  --end <time> \
  --output /tmp/prepared-backtest.db
```

Funded live evidence:

```bash
cargo run -p buba-paint --release -- validate-live-fidelity \
  --db-path <db> \
  --start <time> \
  --end <time> \
  --output /tmp/live-fidelity.json
```

`prepare-backtest-input` opens the source DB read-only, copies the selected interval, preserves CLOB blocks, creates offline replay indexes, runs replay and backtest-input validation, and writes a manifest. Large sweeps should use the prepared DB rather than the append-optimized runtime DB.

## Runtime Metadata

Runtime metadata must not pretend that a run is research-grade before validation. The bot may record:

* configured storage profile
* configured capture capability
* recent observed feed classes
* recent missing classes
* writer lag
* queue depth
* drop/error state
* runtime config snapshot

Offline validators own durable classification fields such as `replay_quality_class`, `backtest_ready`, and `live_fidelity_class`.

## Backtest Semantics

The backtester replays typed feed events at recorded receive time. When microsecond receive timestamps exist, they determine ordering. Otherwise millisecond ordering is used.

Live-runtime DBs may not store derived `open_price` and `close_price` columns. The backtester derives missing open price from the first Binance `aggTrade` inside a market window and missing close price from the last Binance `aggTrade` before close when needed for reporting. Settled outcomes still come from `markets.outcome`; missing outcomes fail validation instead of being guessed.

When replay-grade rows are absent, the backtester can fall back to legacy `tick_data` snapshots. That path is lower fidelity and must not be used as evidence for latency-sensitive parameter selection.

## Live Fidelity

Public feed replay is insufficient for funded live research. A funded interval must also explain:

* strategy feature snapshot
* market/window/open state
* fee, tick, min-size, token, and collateral metadata
* side, order type, requested price, requested size, and requested dollar amount
* client order ID
* submit, acknowledge, update, fill, cancel, and unknown timing
* venue status and raw-safe response fragments
* account snapshots
* reconciliation events
* control audit

The classes are:

* `research_grade_live`: public and private evidence are complete.
* `descriptive_only_live`: live evidence exists but cannot fully explain the interval.
* `no_live_trading`: the interval contains no funded live-trading evidence.

`research_grade_live` still is not a perfect exchange simulator. Queue position, hidden liquidity, matching-engine internals, network path differences, and relayer timing are not fully reconstructable.

## Limits

Do not trust:

* configured replay profile as proof of observed capture quality
* `tick_data` as latency-replay evidence
* old runs without Binance book state for parameter selection
* funded live runs without live-fidelity validation for sweeps
* dashboard charts as a substitute for validators
* runtime DBs as optimized sweep inputs before preparation

## Useful Queries

```sql
SELECT 'feed_events' table_name, COUNT(*) FROM feed_events
UNION ALL SELECT 'clob_replay_blocks', COALESCE(SUM(row_count), 0) FROM clob_replay_blocks
UNION ALL SELECT 'signals', COUNT(*) FROM signals
UNION ALL SELECT 'rejections', COUNT(*) FROM strategy_rejection_summaries
UNION ALL SELECT 'trades', COUNT(*) FROM simulated_trades
UNION ALL SELECT 'results', COUNT(*) FROM trade_results;

SELECT timestamp_ms, market_id, strategy, reason, count, details_json
FROM strategy_rejection_summaries
ORDER BY timestamp_ms DESC
LIMIT 20;

SELECT signal_id, decision_status, rejection_reason,
       order_submitted_at_ms, expected_arrival_at_ms,
       order_processed_at_ms, effective_arrival_delay_ms
FROM signal_metrics
ORDER BY signal_id DESC
LIMIT 20;
```

Historical run quality notes live in [runs.md](./runs.md). Run-specific investigations belong under `data/experiments/...`.
