# CLAUDE.md

AI development guidelines for the buba workspace.

## Project

buba is a paper-trading platform for Polymarket prediction markets. The workspace has four components. paint (`bots/paint`) is the first bot: 5-minute BTC Up/Down, three WebSocket feeds, two strategies, SQLite persistence, a shared live-like execution engine for live paper trading and backtests, compact raw `feed_events` capture for new runs, additive compatibility for legacy-snapshot runs, a shared signal-feature engine, order-book liquidity checks, and dynamic fee modeling. The agent (`agent/`) monitors any bot's database and exposes REST + WebSocket APIs. The dashboard has a Rust backend (`dashboard/server/`) that proxies to agents with JWT auth, and a React frontend (`dashboard/client/`) for the UI. Rust, Cargo workspace, TDD throughout.

## Build & Test

```bash
cargo build                 # debug build
cargo build --release       # release build (use for live/backtest)
cargo test                  # run the Rust test suites
cargo clippy -- -D warnings # lint (must pass with zero warnings)
cargo fmt --all --check     # format check (must pass)
cargo llvm-cov --all-targets --summary-only # line coverage report
make lint                   # fmt + clippy + strict Rust + TS comment audits
make comment-audit          # full Rust rustdoc/comment backlog report
make test-fast              # workspace Rust libs + frontend Vitest
make test-integration       # stable Rust integration suites
make test-slow              # bot live-system suite
make test-e2e               # Playwright browser E2E
make test-all               # fast + integration + slow + browser E2E
make coverage               # Rust coverage summaries + frontend coverage
make coverage-gate          # component coverage floors

cd dashboard/client && npm test         # run the frontend Vitest suite
cd dashboard/client && npm run test:e2e # run browser E2E
cd dashboard/client && npm run test:coverage
```

Example commands:
```bash
cargo run -p buba-paint --release -- backtest --data data/market-data.db --start 2026-02-20 --end 2026-03-04
cargo run -p buba-paint --release -- sweep --data data/market-data.db --start 2026-02-20 --end 2026-03-04 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0005 --output data/sweeps/test/sweep.csv
cargo run -p buba-paint --release -- live --db-path runs/009/buba-paint.db --balance 200
cargo run -p buba-paint --release -- init-db --db-path runs/009/buba-paint.db --balance 200
cargo run -p buba-paint --release -- verify-settlements --db data/market-data.db
cargo run -p buba-paint --release -- latency-probe --timeout-ms 3000
cargo run -p buba-paint --release -- db-footprint --db-path /tmp/paint.db
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
11. After every major update (new module, new feature, schema change, new CLI command), all documentation files (CLAUDE.md, Readme.md, sweep notes) must be fully read and revised. Do not append blindly. Remove or correct stale information. The goal is clean, accurate, reasonably sized documents that let any person or LLM rebuild full project context.

## Documentation Style

When writing or editing `.md` files, code comments, or any prose in this project:

- Do not use em-dashes or double-dashes as separators in prose. Restructure the sentence with periods, commas, colons, or parentheses instead.
- Do not overuse bold (`**text**`). Markdown files must be readable as plain text without a renderer. Reserve bold for genuine warnings only.
- Do not generate ASCII art diagrams. They are never aligned properly and waste tokens. Describe architecture in plain prose.
- Do not use tables for prose content. Tables are acceptable only for actual tabular data (e.g. sweep results with numeric columns). Prefer lists or plain paragraphs.
- Do not use unicode punctuation. Stick to ASCII.
- Do not hard-wrap prose at any column width. Let lines flow naturally.
- Keep it direct. Write like a human engineer, not a marketing page.
- Every Rust function, including private helpers and test-only functions, must have concise `///` rustdoc.
- Do not leave inline comments inside Rust function bodies. Prefer self-explanatory names, extracted helpers, and rustdoc on the function itself.
- Do not use decorative separator comments anywhere in the repo. If a section needs structure, express it through names, types, helpers, or Markdown headers where appropriate.
- When touching a file, review existing comments for staleness. Remove or rewrite stale comments instead of leaving them beside changed behavior.
- `make lint` enforces the strict Rust and TS comment policies across the workspace. Use `make comment-audit` to print the current audit summaries when you need to inspect violations.

## Module Map

### paint bot (`bots/paint/src/`)

Core loop: `cli.rs` (clap CLI parsing, command dispatch, `parse_time`, `init-db`, `upgrade-history`, `latency-probe`), `live.rs` (live trading loop: feeds + discovery + delayed window activation + event-driven strategy evaluation + authoritative settlement via channel), `config.rs` (all env-configurable settings, `set_param` for sweeps), `latency_probe.rs` (operator-facing Gamma/Binance/RTDS/CLOB probe).

Strategies: `strategies/latency_arb.rs` (feature-scored stale odds, adaptive threshold, cooldown), `strategies/spread_capture.rs` (fee-aware two-leg taker spread capture with legging-risk gates), `signal_features.rs` (shared feature engine used by live paper and backtests).

Feeds: `feeds/binance_feed.rs` (combined Binance `aggTrade`, `bookTicker`, and `depth@100ms` stream), `feeds/clob_feed.rs` (Polymarket CLOB order book, best-bid-ask handling, dynamic resubscription), `feeds/chainlink_feed.rs` (Polymarket RTDS Chainlink prices + staleness detection), `feeds/util.rs` (exponential backoff with jitter, stable connection tracking).

Data: `bankroll.rs` (per-strategy half-Kelly sizing, caps, confidence curve, DD pause), `position_manager.rs` (trade lifecycle: open with debug logging on every rejection path, guard duplicates, authoritative resolve), `circuit_breaker.rs` (pause trading after N consecutive losses), `tick_logger.rs` (1s telemetry sampling to SQLite), `trend_tracker.rs` (experimental directional trend filter, off by default), `market_discovery.rs` (Gamma API polling, slug-based window discovery, resolution polling for deferred settlement).

Execution: `executor.rs` (`ExecutionEngine`, shared by live paper trading and backtests, with simulated order latency, partial fills, no-fills, and execution metrics).

Fees and verification: `fees.rs` (historical fee schedule resolution plus Polymarket dynamic taker fee formula: `fee = shares * price * feeRate * (price * (1-price))^exponent`), `verify.rs` (backfill Polymarket resolutions from Gamma API, compare against Chainlink-derived settlements, `verify-settlements` CLI).

SDK integration: `polymarket.rs` (read-only wrapper around the official `polymarket-client-sdk` crate, queries CLOB API for market resolution via `tokens[].winner` field, no trading capability).

Backtesting: `backtest/runner.rs` (core replay loop: replay -> strategies -> execution -> settle), `backtest/sweep.rs` (parallel parameter sweep via rayon, PID-based temp DBs), `backtest/tick_replay.rs` (loads `feed_events` when present and falls back to `tick_data`), `backtest/window_manager.rs` (replays market windows from DB), `backtest/feed_state.rs` (simulated feed state for backtest strategies), `backtest/momentum.rs` (rolling window momentum calculator).

Database: `db/database.rs` (rusqlite wrapper, prepared statements, bounded WAL mode, grouped footprint reporting), `db/schema.rs` (additive column and table migrations via `add_column_if_missing`), `db/build_data.rs` (merges enriched run DBs into `market-data.db`, including optional signal and telemetry tables when present), `db/upgrade_history.rs` (in-place historical upgrade and metadata backfill for runs `004` through `009`).

Shared: `types.rs` (Signal, BookState, MarketWindow, TradeResult, replay fidelity, feed-event structs, signal telemetry), `clock.rs` (Clock trait + SystemClock + BacktestClock), `errors.rs` (thiserror error types).

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

Do not edit files in `runs/` manually. The only supported in-place mutation is the additive `upgrade-history` workflow for historical run DBs.

`data/` contains derived data (sweeps, experiments, merged DB, backfill cache), all reproducible from `runs/` data. Safe to regenerate, but valuable to keep.

## Testing Practices

Test inventory changes frequently. Use `cargo test`, `make test-all`, and the frontend test commands as the current source of truth instead of relying on hard-coded counts.

- paint bot: Rust unit tests live under `src/*/tests/` via `#[path]` and retain access to private internals with `use super::*`. Coverage emphasis is on replay, execution, fees, accounting, migrations, feeds, and strategy logic. Integration tests in `tests/` use mock WebSocket servers and wiremock. The slow live-system suite includes circuit-breaker recovery within the same bot run.
- Coverage gates now measure Rust library and integration-test coverage while explicitly excluding thin `main.rs` bootstrapping entrypoints. Floors are currently `80%` for `buba-paint`, `90%` for `buba-agent`, `90%` for `buba-dashboard`, and `80%` for the frontend.
- agent: Rust tests cover REST endpoints, WebSocket polling/broadcast, DB reader compatibility for legacy/new schemas, process manager, auth middleware, and integration round-trips.
- dashboard server: Rust tests cover auth/JWT, bot proxy handlers, WebSocket proxy, config parsing, user management, error mapping, and degraded proxy behavior through integration tests.
- dashboard client: Vitest uses @testing-library/react + jsdom. Current network boundary supports both focused `vi.mock(...)` unit tests and MSW-backed fetch tests. Setup stays in `src/test/setup.ts`; shared MSW server lives in `src/test/msw-server.ts`.
- Browser E2E: Playwright lives in `dashboard/client/e2e/` with a mocked API/WebSocket harness. Current smoke flows cover login/navigation/session persistence and 401 redirect handling.
- Comment policy: `tools/rust-comment-policy/` parses Rust syntax, requires rustdoc on every Rust function, and rejects non-doc Rust comments. `scripts/ts_comment_audit.mjs` rejects non-directive comments in the dashboard TypeScript code.
- Sweep parity from the old snapshot simulator is no longer the goal. After the live-like simulator rewrite, validate schema compatibility, execution metrics, and whether legacy-snapshot replay remains deterministic across repeated runs.
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
- Schema evolution uses `add_column_if_missing` for backward compatibility with old run DBs
- Never create temporary or test databases inside the project directory. Use `/tmp/` or `tempfile::NamedTempFile` in tests. Stray `.db` files in the project root pollute git status and risk accidental commits.

## Key Behavioral Constraints

- Tick grouping: raw `feed_events` replay at exact timestamps. When microsecond fields exist, replay orders by `received_at_us`; otherwise it falls back to millisecond ordering. Legacy `tick_data` fallback groups within a 10ms tolerance window.
- Kelly criterion: half-Kelly, per-strategy, rolling 30-trade window
- Confidence curve: `max(0.0, (confidence - 0.5) * 2.5)`
- Settlement: binary (1.0 or 0.0), close_price >= open_price means UP wins
- Window activation: market_discovery discovers both current and next 5-minute slots. The live loop only activates a window (sets current_window, resubscribes CLOB, captures open_price) when its start_time has passed. Future windows are stored in known_windows and scheduled for delayed activation via a tokio channel. This prevents trading against a window that hasn't started yet.
- Settlement: record a provisional estimate for observability if needed, but only authoritative Polymarket outcomes may update bankroll, Kelly state, trend tracking, or circuit-breaker state.
- Polymarket resolution timing: markets resolve 40 seconds to 4+ minutes after the nominal 5-minute window close. The Gamma API `outcomePrices` field converges to `[1,0]` or `[0,1]` only after the on-chain Chainlink resolution completes. Polling starts after `RESOLUTION_INITIAL_DELAY_MS` (default 30s), then retries every `RESOLUTION_POLL_DELAY_MS` (default 10s) up to `RESOLUTION_POLL_RETRIES`.
- Execution: latency-arb and spread-capture both queue FAK-style paper orders with simulated arrival latency. Partial fills are allowed. Spread legs are independent, so one-sided residual exposure is possible.
- Liquidity clamping: trade sizes are capped to available order-book depth plus a hard USD cap (`MAX_POSITION_USD`, default $500).
- Dynamic fees: Polymarket charges taker fees on crypto markets using formula `fee = shares * price * feeRate * (price * (1-price))^exponent`. Current live crypto defaults are feeRate=0.072 and exponent=1 as of March 30, 2026. Historical replays can resolve pre-change and post-change schedules by date unless explicitly overridden.
- DD pause hysteresis: after pause expires, DD must recover below `peak_dd_pause_pct - dd_pause_recovery_pct` (default 25%) before re-arming.
- Feed reconnection: backoff only resets if connection lasted > `reconnect_min_stable_ms` (default 5s). Prevents reconnect storms.

## Key Implementation Details

- Market discovery slug: `btc-updown-5m-{floor(unix_seconds/300)*300}`. The generic `/markets` endpoint does NOT return these 5-minute markets. Discovery fetches both current_slot and next_slot, but only current_slot is activated immediately.
- CLOB initial book: array of `{asset_id, bids, asks}` (no `event_type`). Subsequent updates: `{event_type: "price_change", price_changes: [...]}`.
- Chainlink RTDS: initial dump `{payload:{data:[...]}}` (no `topic`), then live `{topic:"crypto_prices_chainlink", payload:{...}}`.
- Binance live capture: the default market-data URL now combines `aggTrade`, `bookTicker`, and `depth@100ms`, and requests microsecond timestamps when Binance exposes them.
- Live storage profile: `FEED_EVENT_STORAGE_PROFILE=compact` is the production default. It keeps typed replay fields, suppresses `bookTicker` persistence, buckets Binance depth to 250ms summaries, coalesces `CLOB` top-of-book writes, and drops bulky hot-path payload blobs. `full_debug` is only for short local diagnostics.
- No-signal diagnostics: live runs aggregate explicit strategy rejection reasons into `strategy_rejection_summaries` and concise structured log rollups. Inspect those before guessing a retune when a run shows zero signals. The DB keeps the full JSON detail; the normal log should stay operator-readable.
- Spread synchronization: spread-capture must not evaluate a binary market from mixed-time `UP` and `DOWN` books. Persist and inspect per-leg effective timestamps plus `inter_leg_skew_ms`, and reject the setup as `legs_out_of_sync` when the skew exceeds `SPREAD_CAPTURE_MAX_LEG_SKEW_MS`.
- Spread activation: the churn gate is configurable. `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S` should be treated as a live-regime calibration knob, not a hard-coded invariant.
- Spread sizing: `SPREAD_CAPTURE_MAX_POSITION_FRACTION` is the spread-only balance cap. If it is unset, spread sizing falls back to `MAX_POSITION_FRACTION`. Keep latency-arb on the global cap unless a future change explicitly separates it further.
- Restart continuity: when a bot restarts over an existing live DB, the active window should recover its open price from the earliest persisted Binance tick inside that window before falling back to the current in-memory price.
- Live CLOB freshness: trading freshness must use observed local receipt time when `best_bid_ask` / `book` / `price_change` messages omit a usable source timestamp. Preserve the raw source timestamp in `feed_events`, but do not let `event_at_ms=0` poison quote age. For binary markets, combined quote freshness must reflect the older side of the two books, not the freshest side.
- Live CLOB liquidity: direct `best_bid_ask` payloads can omit size fields. Missing sizes must preserve the current in-memory same-side liquidity instead of overwriting it with `0.0`. Only explicit empty or non-positive quotes should clear size.
- Execution freshness: the paper executor and strategy layer must use the same effective book timestamp helper, `max(observed_at_ms, raw_timestamp_ms)`, so a fresh observed quote does not fail fills only because the source timestamp was absent.
- Execution telemetry: queued orders should not remain stuck at `submitted` after their arrival has been processed. Update `signal_metrics` to `filled` or `missed`, record `order_processed_at_ms` / `order_processed_at_us`, and persist `effective_arrival_delay_ms` for later latency calibration.
- Queue diagnostics: pre-submit queue rejections must be explicit. Do not collapse them into a generic `position_limits` bucket. Distinguish `max_open_positions`, `duplicate_open_position`, and `duplicate_pending_order`, and prefer `signal queued` / `batch queued` logs over `signal generated` messages that imply execution progress before the queue accepts the order.
- Submit-time sizing: single-side orders size from the current entry ask but reserve capital against the worst-case limit price. Enforce `MIN_BET_USD` and venue min-size checks before queueing so sub-minimum orders fail as `below_min_bet_on_submit` or `below_market_min_size_on_submit`, not as guaranteed misses at arrival.
- Simulated order latency: `SIM_ORDER_LATENCY_MS` is a paper-arrival assumption, not a measured Polymarket venue latency. Keep it explicit in docs, notes, and live run reasoning.
- Agent secret: `AGENT_SECRET` is required for normal agent startup. Missing or blank values must fail fast instead of silently falling back to an empty secret and creating a broken dashboard login loop.
- Staleness: if no Chainlink data for `CHAINLINK_STALE_MS`, force-reconnect. During staleness, settlement falls back to Binance price.
- Momentum: `(latest - oldest) / oldest` over rolling window. Guarded against division by zero (oldest price <= 0 returns 0).
- Opposing position guard: single signals block same-strategy same-window. Batch signals (spread-capture) only block exact duplicates. All rejections are logged at debug level with the reason.
- Deferred resolution: background tokio task polls Gamma API `/events/slug/{slug}` starting 30s after window close (10s intervals, up to 5 min). Sends `DeferredResolution` message to the main loop via `tokio::sync::mpsc`. Main loop settles only once on the authoritative outcome.
- Gamma API `outcomePrices` is a JSON-encoded string (not a JSON array). Parse with `serde_json::from_str::<Vec<String>>`.
- Official Polymarket Rust SDK (`polymarket-client-sdk` v0.4) integrated for read-only CLOB queries. `Client::new(host, Config::default())` creates unauthenticated client. `client.market(condition_id)` returns `MarketResponse` with `tokens[].winner: bool`.
- `ExecutionEngine` is the shared paper execution path for live trading and backtests. Future live order placement must plug into that model rather than reintroducing a second simulator.
- `init-db` CLI command creates an empty database with all tables and an initial balance event, without starting the bot. Use this instead of running and killing the bot to seed the DB for the agent.

## Common Issues

- "No active 5-min BTC market found": between windows or Gamma API hiccup. Retries automatically.
- No signals generated: inspect `strategy_rejection_summaries` first, then read the concise `strategy rejection rollup` log lines. The dominant reason should tell you whether the blocker is spread threshold, stale features, expected edge, or direction selection. Do not retune blindly.
- "Balance below minimum": drawdown hit safety limit. Increase `STARTING_BALANCE`.
- Chainlink feed stale: RTDS stopped sending while WS stays open. Auto-detected, force-reconnects.
- Sweep results differ between runs: stale temp DBs. PID-based paths prevent this; check `/tmp/`.
- Trades open but never settle: window lifecycle race. Fixed in v0.5 with `known_windows` HashMap.
- DD pause loops forever: no hysteresis on re-trigger. Fixed: `dd_pause_armed` + recovery pct.
- Feed reconnect storm (100s/hour): backoff resets on short connects. Fixed: `should_reset_backoff` in util.rs.
- Gamma resolution polling exhausted: Polymarket takes 40s-4min to resolve. If polling is exhausted, leave the window unresolved rather than mutating performance state with a provisional settlement.
- Legacy replay limitation: old runs only have 1 Hz top-of-book snapshots. Do not describe those replays as true latency-arb reconstruction.
- Signal generated but no trade: check debug logs for the rejection reason. Every guard in `try_open` logs why it rejected. Common causes: duplicate position in the same market, insufficient balance, below min bet after liquidity clamp.
- Signal rejected before queue: inspect `signal_metrics.rejection_reason` first. Common causes are explicit queue-state blocks (`duplicate_pending_order`, `duplicate_open_position`, `max_open_positions`) or submit-time sizing failures (`below_min_bet_on_submit`, `below_market_min_size_on_submit`).
- Agent instructions alias: `AGENTS.md` should stay as a tiny compatibility alias that points to `CLAUDE.md`. Edit `CLAUDE.md`, not `AGENTS.md`, when the canonical repo instructions change.
- Signals stay at `submitted` with no trades: inspect `signal_metrics.decision_status`, `rejection_reason`, and the concise `paper order missed` / `paper execution rollup` logs. Common causes: stale book on arrival, zero preserved liquidity, or limit price not crossing when the delayed paper order reaches the book.
- Trades assigned to wrong market window: fixed in v0.8.1. Previous versions set current_window to next_slot immediately when discovered (3-5 min early). Now windows are only activated at their start_time via delayed tokio task.

## Useful SQL Queries

```sql
-- Row counts per table
SELECT 'tick_data' t, COUNT(*) FROM tick_data
UNION SELECT 'markets', COUNT(*) FROM markets
UNION SELECT 'signals', COUNT(*) FROM signals
UNION SELECT 'rejections', COUNT(*) FROM strategy_rejection_summaries
UNION SELECT 'trades', COUNT(*) FROM simulated_trades
UNION SELECT 'results', COUNT(*) FROM trade_results
UNION SELECT 'balance', COUNT(*) FROM balance_log;

-- Recent no-signal summaries
SELECT timestamp_ms, market_id, strategy, reason, count, details_json
FROM strategy_rejection_summaries
ORDER BY timestamp_ms DESC
LIMIT 20;

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

-- Settlement corrections
SELECT trade_id, settlement_status, provisional_pnl, pnl_net
FROM trade_results WHERE settlement_status = 'corrected';

-- Check which window a trade was assigned to vs the correct window
SELECT t.id, t.side,
  datetime(t.timestamp/1000, 'unixepoch') as trade_time,
  m.slug as assigned_window,
  (SELECT m2.slug FROM markets m2
   WHERE m2.start_time <= t.timestamp AND m2.end_time > t.timestamp
   LIMIT 1) as correct_window
FROM simulated_trades t
JOIN markets m ON t.market_id = m.market_id;
```

## Cross-compilation

- Dev: macOS aarch64 (Apple Silicon)
- Prod: Linux aarch64 (AWS t4g.small, Ubuntu 24.04)
