# CLAUDE.md -- AI Development Guidelines

## Project

Polymarket 5-minute BTC Up/Down paper trading bot. Rust, single binary,
three WebSocket feeds, two strategies, SQLite persistence. 569 tests,
91.4% line coverage, TDD throughout.

## Build & Test

```bash
cargo build                        # debug build
cargo build --release              # release build (use for live/backtest)
cargo test                         # run all 569 tests
cargo clippy -- -D warnings        # lint (must pass with zero warnings)
cargo fmt --check                  # format check (must pass)
cargo llvm-cov --all-targets --summary-only  # line coverage report
```

Example commands:
```bash
cargo run --release -- backtest --data data/market-data.db --start 2026-02-20 --end 2026-03-04
cargo run --release -- sweep --data data/market-data.db --start 2026-02-20 --end 2026-03-04 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0005 --output data/sweeps/test/sweep.csv
cargo run --release -- live --db-path runs/008/buba-paint.db --balance 200
```

## Architecture Rules

1. **No `unwrap()` or `expect()` in library code** -- use `anyhow::Result` or `thiserror`.
   Acceptable only in tests and in `main()`.
2. **All floating-point values use `f64`** -- never `f32`.
3. **Config is immutable after construction** -- pass `&Config` everywhere.
4. **Clock is injectable** -- `Clock` trait with `SystemClock` and `BacktestClock` impls.
5. **Database layer owns all SQL** -- no raw SQL strings outside `src/db/`.
6. **Strategies are stateful structs** implementing the `Strategy` trait.
7. **Unit tests live in `src/*/tests/`** via `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`.
   Integration tests live in the top-level `tests/` directory.
8. **TDD strictly enforced** -- write tests before implementation.
9. **When a test fails, fix the code, not the test.** The only exception is if the
   test itself has a wrong expected value -- document why before changing it.

## Module Map

| File                             | Responsibility                                              |
| -------------------------------- | ----------------------------------------------------------- |
| `cli.rs`                         | CLI parsing (clap), command dispatch, `parse_time`          |
| `live.rs`                        | Live trading loop: feeds + discovery + strategies + settle  |
| `config.rs`                      | All env-configurable settings, `set_param` for sweeps       |
| `bankroll.rs`                    | Per-strategy half-Kelly sizing, caps, confidence, DD pause  |
| `position_manager.rs`            | Trade lifecycle: open, guard duplicates, resolve at close   |
| `circuit_breaker.rs`             | Pause trading after N consecutive losses                    |
| `trend_tracker.rs`               | Experimental directional trend filter (off by default)      |
| `market_discovery.rs`            | Gamma API polling, slug-based window discovery              |
| `tick_logger.rs`                 | 1s interval tick sampling to SQLite                         |
| `types.rs`                       | Shared data structures: Signal, BookState, MarketWindow     |
| `clock.rs`                       | Clock trait + SystemClock + BacktestClock                   |
| `errors.rs`                      | thiserror error types                                       |
| `feeds/binance_feed.rs`          | Binance aggTrade WebSocket stream                           |
| `feeds/chainlink_feed.rs`        | Polymarket RTDS Chainlink prices + staleness detection      |
| `feeds/clob_feed.rs`             | Polymarket CLOB order book + dynamic resubscription         |
| `feeds/util.rs`                  | Exponential backoff with jitter, stable connection tracking |
| `strategies/latency_arb.rs`      | Momentum vs stale odds, adaptive threshold, cooldown        |
| `strategies/spread_capture.rs`   | UP ask + DOWN ask < threshold, buys both sides              |
| `backtest/runner.rs`             | Core replay loop: ticks -> strategies -> trades -> settle   |
| `backtest/sweep.rs`              | Parallel parameter sweep (rayon), PID-based temp DBs        |
| `backtest/tick_replay.rs`        | Loads tick_data, groups by 10ms tolerance                   |
| `backtest/window_manager.rs`     | Replays market windows from DB                              |
| `backtest/feed_state.rs`         | Simulated feed state for backtest strategies                |
| `backtest/momentum.rs`           | Rolling window momentum calculator                          |
| `db/database.rs`                 | rusqlite wrapper, prepared statements, WAL mode             |
| `db/schema.rs`                   | 6 SQLite tables with indexes                                |
| `db/build_data.rs`               | Merges run DBs into market-data.db (`build-data` CLI)       |

## Data Preservation

**`runs/` contains irreplaceable primary data** from live paper trading
sessions -- DB files, bot logs, analysis PNGs. This data was collected
over weeks of real-time trading and cannot be regenerated.

**NEVER delete, overwrite, or modify files in `runs/`.**

`data/` contains derived data (sweeps, experiments, merged DB) -- all
reproducible from `runs/` data. Safe to regenerate, but valuable to keep.

## Testing Practices

- **Unit tests**: `src/*/tests/` directories, accessed via `#[path]` attribute.
  Tests retain full access to private internals via `use super::*`.
- **Integration tests**: `tests/` directory (backtest, feeds, discovery,
  live system, sweep, CLI). Use mock WebSocket servers (`tests/support/mock_ws.rs`)
  and wiremock for HTTP mocking.
- **Coverage**: 569 tests, 91.4% line coverage. Run `cargo llvm-cov`.
- **Sweep parity**: rust-005 == rust-004 byte-for-byte (excluding elapsed_s).
  rust-006 is the full-range sweep (Feb 15 -- Mar 20, 8.8M ticks).
  Always verify after changes.
- **Sweep temp DBs**: PID-based paths (`/tmp/buba-sweep-{pid}-NNNN.db`) to
  prevent stale-data contamination between runs.

## Naming Conventions

- Snake_case for files and functions (Rust standard)
- Types: `TickGroup`, `BookState`, `SignalDirection` (PascalCase)
- Config fields: `latency_arb_momentum_threshold` (snake_case struct fields)
- Modules: `backtest::runner`, `strategies::latency_arb`, `db::database`

## Float Precision

- Use `f64` everywhere for prices, balances, fractions
- Integer token counts: cast with `as i64` after `.floor()`
- Sweep CSV output: raw f64 (no rounding)

## SQLite Specifics

- Always use WAL mode + NORMAL synchronous
- Prepared statements via `conn.prepare_cached()`
- Transactions via `conn.transaction()` for multi-statement atomics
- `data/market-data.db` (merged ticks) has a different schema from per-run DBs

## Key Behavioral Constraints

- Tick grouping: 10ms tolerance window
- Kelly criterion: half-Kelly, per-strategy, rolling 30-trade window
- Confidence curve: `max(0.0, (confidence - 0.5) * 2.5)`
- Settlement: binary (1.0 or 0.0), close_price >= open_price means UP wins
- DD pause hysteresis: after pause expires, DD must recover below
  `peak_dd_pause_pct - dd_pause_recovery_pct` (default 25%) before re-arming.
- Feed reconnection: backoff only resets if connection lasted >
  `reconnect_min_stable_ms` (default 5s). Prevents reconnect storms.

## Key Implementation Details

- Market discovery slug: `btc-updown-5m-{floor(unix_seconds/300)*300}`.
  The generic `/markets` endpoint does NOT return these 5-minute markets.
- CLOB initial book: array of `{asset_id, bids, asks}` (no `event_type`).
  Subsequent updates: `{event_type: "price_change", price_changes: [...]}`.
- Chainlink RTDS: initial dump `{payload:{data:[...]}}` (no `topic`),
  then live `{topic:"crypto_prices_chainlink", payload:{...}}`.
- Staleness: if no Chainlink data for `CHAINLINK_STALE_MS`, force-reconnect.
  During staleness, settlement falls back to Binance price.
- Momentum: `(latest - oldest) / oldest` over rolling window. Guarded
  against division by zero (oldest price <= 0 returns 0).
- Opposing position guard: single signals block same-strategy same-window.
  Batch signals (spread-capture) only block exact duplicates.

## Common Issues

| Symptom                              | Cause                            | Fix                                       |
| ------------------------------------ | -------------------------------- | ----------------------------------------- |
| "No active 5-min BTC market found"   | Between windows or API hiccup    | Retries automatically                     |
| No signals generated                 | Thresholds too tight             | Lower `LATENCY_ARB_MOMENTUM_THRESHOLD`    |
| "Balance below minimum"              | Drawdown hit safety limit        | Increase `STARTING_BALANCE`               |
| Chainlink feed stale                 | RTDS stopped sending, WS open    | Auto-detected, force-reconnects           |
| Sweep results differ between runs    | Stale temp DBs                   | PID-based paths prevent; check `/tmp/`    |
| Trades open but never settle         | Window lifecycle race            | Fixed: `known_windows` HashMap in live.rs |
| DD pause loops forever               | No hysteresis on re-trigger      | Fixed: `dd_pause_armed` + recovery pct    |
| Feed reconnect storm (100s/hour)     | Backoff resets on short connects | Fixed: `should_reset_backoff` in util.rs  |

## Useful SQL Queries

```sql
-- Row counts per table
SELECT 'tick_data' t, COUNT(*) FROM tick_data
UNION SELECT 'markets', COUNT(*) FROM markets
UNION SELECT 'signals', COUNT(*) FROM signals
UNION SELECT 'trades', COUNT(*) FROM simulated_trades
UNION SELECT 'results', COUNT(*) FROM trade_results
UNION SELECT 'balance', COUNT(*) FROM balance_log;

-- Trade results by strategy
SELECT t.strategy, COUNT(*) trades,
  SUM(CASE WHEN r.pnl_0pct > 0 THEN 1 ELSE 0 END) wins,
  ROUND(SUM(r.pnl_0pct), 2) total_pnl
FROM trade_results r
JOIN simulated_trades t ON r.trade_id = t.id
GROUP BY t.strategy;

-- Bankroll curve
SELECT datetime(timestamp/1000, 'unixepoch') AS time,
  event, ROUND(balance, 2) AS balance
FROM balance_log ORDER BY timestamp;
```

## Cross-compilation

- Dev: macOS aarch64 (Apple Silicon)
- Prod: Linux aarch64 (AWS t4g.micro, Ubuntu 24.04)
