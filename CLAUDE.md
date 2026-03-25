# CLAUDE.md

AI development guidelines for the buba workspace.

## Project

buba is a paper-trading platform for Polymarket prediction markets. The workspace has four components. paint (`bots/paint`) is the first bot: 5-minute BTC Up/Down, three WebSocket feeds, two strategies, SQLite persistence. The agent (`agent/`) monitors any bot's database and exposes REST + WebSocket APIs. The dashboard has a Rust backend (`dashboard/server/`) that proxies to agents with JWT auth, and a React frontend (`dashboard/client/`) for the UI. Rust, Cargo workspace, TDD throughout. 874 tests (743 Rust + 131 frontend).

## Build & Test

```bash
cargo build                 # debug build
cargo build --release       # release build (use for live/backtest)
cargo test                  # run all 743 Rust tests
cargo clippy -- -D warnings # lint (must pass with zero warnings)
cargo fmt --all --check     # format check (must pass)
cargo llvm-cov --all-targets --summary-only # line coverage report

cd dashboard/client && npm test # run all 131 frontend tests (vitest)
```

Example commands:
```bash
cargo run -p buba-paint --release -- backtest --data data/market-data.db --start 2026-02-20 --end 2026-03-04
cargo run -p buba-paint --release -- sweep --data data/market-data.db --start 2026-02-20 --end 2026-03-04 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0005 --output data/sweeps/test/sweep.csv
cargo run -p buba-paint --release -- live --db-path runs/008/paint.db --balance 200
```

## Architecture Rules

1. No `unwrap()` or `expect()` in library code. Use `anyhow::Result` or `thiserror`. Acceptable only in tests and in `main()`.
2. All floating-point values use `f64`, never `f32`.
3. Config is immutable after construction. Pass `&Config` everywhere.
4. Clock is injectable. `Clock` trait with `SystemClock` and `BacktestClock` impls.
5. Database layer owns all SQL. No raw SQL strings outside `src/db/`.
6. Strategies are stateful structs implementing the `Strategy` trait.
7. Unit tests live in `src/*/tests/` via `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`. Integration tests live in the top-level `tests/` directory.
8. TDD strictly enforced. Write tests before implementation.
9. When a test fails, fix the code, not the test. The only exception is if the test itself has a wrong expected value; document why before changing it.
10. Frontend tests use vitest + @testing-library/react, colocated in `__tests__/` directories next to the source files they test.

## Documentation Style

When writing or editing `.md` files, code comments, or any prose in this project:

- Do not use em-dashes or double-dashes as separators in prose. Restructure the sentence with periods, commas, colons, or parentheses instead.
- Do not overuse bold (`**text**`). Markdown files must be readable as plain text without a renderer. Reserve bold for genuine warnings only.
- Do not generate ASCII art diagrams. They are never aligned properly and waste tokens. Describe architecture in plain prose.
- Do not use tables for prose content. Tables are acceptable only for actual tabular data (e.g. sweep results with numeric columns). Prefer lists or plain paragraphs.
- Do not use unicode punctuation. Stick to ASCII.
- Do not hard-wrap prose at any column width. Let lines flow naturally.
- Keep it direct. Write like a human engineer, not a marketing page.

## Module Map

### paint bot (`bots/paint/src/`)

Core loop: `cli.rs` (clap CLI parsing, command dispatch, `parse_time`), `live.rs` (live trading loop: feeds + discovery + strategies + settlement), `config.rs` (all env-configurable settings, `set_param` for sweeps).

Strategies: `strategies/latency_arb.rs` (momentum vs stale odds, adaptive threshold, cooldown), `strategies/spread_capture.rs` (UP ask + DOWN ask < threshold, buys both sides).

Feeds: `feeds/binance_feed.rs` (Binance aggTrade WebSocket stream), `feeds/clob_feed.rs` (Polymarket CLOB order book + dynamic resubscription), `feeds/chainlink_feed.rs` (Polymarket RTDS Chainlink prices + staleness detection), `feeds/util.rs` (exponential backoff with jitter, stable connection tracking).

Data: `bankroll.rs` (per-strategy half-Kelly sizing, caps, confidence curve, DD pause), `position_manager.rs` (trade lifecycle: open, guard duplicates, resolve at close), `circuit_breaker.rs` (pause trading after N consecutive losses), `tick_logger.rs` (1s interval tick sampling to SQLite), `trend_tracker.rs` (experimental directional trend filter, off by default), `market_discovery.rs` (Gamma API polling, slug-based window discovery).

Backtesting: `backtest/runner.rs` (core replay loop: ticks -> strategies -> trades -> settle), `backtest/sweep.rs` (parallel parameter sweep via rayon, PID-based temp DBs), `backtest/tick_replay.rs` (loads tick_data, groups by 10ms tolerance), `backtest/window_manager.rs` (replays market windows from DB), `backtest/feed_state.rs` (simulated feed state for backtest strategies), `backtest/momentum.rs` (rolling window momentum calculator).

Database: `db/database.rs` (rusqlite wrapper, prepared statements, WAL mode), `db/schema.rs` (6 SQLite tables with indexes), `db/build_data.rs` (merges run DBs into market-data.db, `build-data` CLI).

Shared: `types.rs` (Signal, BookState, MarketWindow), `clock.rs` (Clock trait + SystemClock + BacktestClock), `errors.rs` (thiserror error types).

### agent (`agent/src/`)

`api.rs` (10 REST endpoints + WS route), `db_reader.rs` (read-only SQLite connection, status/trades/balance/signals/stats queries), `ws.rs` (DB poller + WebSocket broadcast handler), `process_manager.rs` (ChildProcessManager for bot lifecycle control, NoopProcessManager for monitoring-only mode), `auth.rs` (shared-secret Bearer middleware), `types.rs` (BotStatus, TradeRow, WsMessage), `error.rs` (AgentError with HTTP status mapping).

### dashboard server (`dashboard/server/src/`)

`auth.rs` (Argon2 hashing, JWT creation/validation, auth middleware), `config.rs` (TOML config: server port, JWT secret, agents list), `db.rs` (SQLite users/sessions store), `proxy.rs` (HTTP proxy helpers for agent communication), `error.rs` (DashboardError with HTTP status mapping).

API routes: `api/auth_routes.rs` (login, me), `api/bots.rs` (list bots, proxy status/trades/balance/signals/stats/logs/process/start/stop/restart), `api/users.rs` (admin-only user management), `api/ws_proxy.rs` (WebSocket proxy: validates JWT from query param, bridges client <-> agent WebSockets).

### dashboard client (`dashboard/client/src/`)

Pages: `pages/login.tsx` (form-based login), `pages/dashboard.tsx` (stat cards, mini equity chart, open trades, recent activity), `pages/equity.tsx` (full-height equity curve), `pages/trades.tsx` (paginated trade table), `pages/signals.tsx` (signal log), `pages/logs.tsx` (ANSI-colored bot logs with auto-scroll), `pages/stats.tsx` (per-strategy breakdown).

Hooks: `hooks/use-auth.ts` (login, logout, session restore), `hooks/use-bot-status.ts`, `hooks/use-trades.ts`, `hooks/use-balance.ts`, `hooks/use-signals.ts`, `hooks/use-logs.ts`, `hooks/use-process-status.ts` (all TanStack React Query wrappers), `hooks/use-live-updates.ts` (WebSocket -> React Query cache invalidation).

Lib: `lib/api.ts` (typed REST client, Bearer token, auto-clear on 401), `lib/ws.ts` (WebSocket with backoff retry, max 3 attempts), `lib/types.ts` (shared TypeScript interfaces), `lib/utils.ts` (formatUsd, formatPct, formatTime, cn, pnlColor).

Components: `components/layout/app-shell.tsx` (sidebar + header + outlet, bot selection via sessionStorage), `components/layout/header.tsx` (bot status, process controls), `components/layout/nav.tsx` (bot list + page links), `components/layout/logo.tsx`, `components/common/protected-route.tsx` (auth guard), `components/common/loading.tsx` (spinner), `components/dashboard/stat-card.tsx`, `components/dashboard/mini-chart.tsx`, `components/dashboard/open-trades.tsx`, `components/dashboard/recent-activity.tsx`, `components/equity/equity-chart.tsx`, `components/trades/trade-table.tsx`, `components/signals/signal-table.tsx`.

Store: `stores/auth-store.ts` (Zustand, token + user, persisted to localStorage).

## Data Preservation

`runs/` contains irreplaceable primary data from live paper trading sessions: DB files, bot logs, analysis PNGs. This data was collected over weeks of real-time trading and cannot be regenerated.

NEVER delete, overwrite, or modify files in `runs/`.

`data/` contains derived data (sweeps, experiments, merged DB), all reproducible from `runs/` data. Safe to regenerate, but valuable to keep.

## Testing Practices

874 tests total: 743 Rust + 131 frontend.

- paint bot: 569 tests (517 unit + 52 integration). Unit tests in `src/*/tests/` directories, accessed via `#[path]` attribute. Tests retain full access to private internals via `use super::*`. Integration tests in `tests/` directory using mock WebSocket servers (`tests/support/mock_ws.rs`) and wiremock for HTTP mocking.
- agent: 92 tests (89 unit + 3 integration). Covers REST endpoints, WebSocket polling/broadcast, DB reader, process manager (child + noop), auth middleware, error mapping.
- dashboard server: 82 tests (80 unit + 2 integration). Covers auth/JWT, bot proxy handlers, WebSocket proxy, config parsing, user management, error mapping. Integration tests use wiremock as mock agent.
- dashboard client: 131 tests across 27 files. Uses vitest + @testing-library/react + jsdom. Key patterns: `vi.mock("../../lib/api")` for API module mocking, `renderWithProviders` wrapper (QueryClientProvider + MemoryRouter), `useAuthStore.getState()`/`setState()` for Zustand testing, `MockWebSocket` class for WebSocket tests, `vi.useFakeTimers()` for reconnection logic. Setup: `src/test/setup.ts` (localStorage polyfill + jest-dom matchers). Shared wrapper: `src/test/test-utils.tsx`.
- Sweep parity: rust-005 == rust-004 byte-for-byte (excluding elapsed_s). rust-006 is the full-range sweep (Feb 15 to Mar 20, 8.8M ticks). Always verify after changes.
- Sweep temp DBs: PID-based paths (`/tmp/buba-sweep-{pid}-NNNN.db`) to prevent stale-data contamination between runs.

## Naming Conventions

- Snake_case for files and functions (Rust standard)
- Types: `TickGroup`, `BookState`, `SignalDirection` (PascalCase)
- Config fields: `latency_arb_momentum_threshold` (snake_case struct fields)
- Modules: `backtest::runner`, `strategies::latency_arb`, `db::database`
- Frontend: kebab-case files (`use-auth.ts`), PascalCase components (`AppShell`)

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
- DD pause hysteresis: after pause expires, DD must recover below `peak_dd_pause_pct - dd_pause_recovery_pct` (default 25%) before re-arming
- Feed reconnection: backoff only resets if connection lasted > `reconnect_min_stable_ms` (default 5s). Prevents reconnect storms.

## Key Implementation Details

- Market discovery slug: `btc-updown-5m-{floor(unix_seconds/300)*300}`. The generic `/markets` endpoint does NOT return these 5-minute markets.
- CLOB initial book: array of `{asset_id, bids, asks}` (no `event_type`). Subsequent updates: `{event_type: "price_change", price_changes: [...]}`.
- Chainlink RTDS: initial dump `{payload:{data:[...]}}` (no `topic`), then live `{topic:"crypto_prices_chainlink", payload:{...}}`.
- Staleness: if no Chainlink data for `CHAINLINK_STALE_MS`, force-reconnect. During staleness, settlement falls back to Binance price.
- Momentum: `(latest - oldest) / oldest` over rolling window. Guarded against division by zero (oldest price <= 0 returns 0).
- Opposing position guard: single signals block same-strategy same-window. Batch signals (spread-capture) only block exact duplicates.

## Common Issues

- "No active 5-min BTC market found": between windows or Gamma API hiccup. Retries automatically.
- No signals generated: thresholds too tight. Lower `LATENCY_ARB_MOMENTUM_THRESHOLD`.
- "Balance below minimum": drawdown hit safety limit. Increase `STARTING_BALANCE`.
- Chainlink feed stale: RTDS stopped sending while WS stays open. Auto-detected, force-reconnects.
- Sweep results differ between runs: stale temp DBs. PID-based paths prevent this; check `/tmp/`.
- Trades open but never settle: window lifecycle race. Fixed with `known_windows` HashMap in live.rs.
- DD pause loops forever: no hysteresis on re-trigger. Fixed with `dd_pause_armed` + recovery pct.
- Feed reconnect storm (100s/hour): backoff resets on short connects. Fixed with `should_reset_backoff` in util.rs.

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
