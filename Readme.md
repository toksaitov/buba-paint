# buba

Paper-trading and backtesting platform for Polymarket prediction markets. `paint` is the first bot: a 5-minute BTC Up/Down strategy stack that consumes Binance market data, the Polymarket CLOB, and Chainlink RTDS settlement prices. Live paper trading and backtesting now share the same live-like execution model: event-driven strategy evaluation, simulated order-arrival latency, partial fills, min-size and tick-size checks, replay-grade feed-event capture for new research runs, a shared signal-feature engine, a shared evaluation/submission path, and a portfolio router that keeps the strategy families from competing for the same market snapshot. Historical runs `004` through `009` can be upgraded in place with additive metadata and replay tables so the backtester can replay one canonical event stream while still merging older legacy data with future richer runs.

The local tree now also contains real-money readiness scaffolding for a future small-bankroll proxy-wallet pilot. Execution modes are explicit (`paper`, `live_readonly`, `live_trading`), market discovery persists live fee and venue metadata, additive live ledger tables exist, the agent and dashboard expose live-readiness and reconciliation surfaces, and a local TypeScript sidecar provides the authenticated Polymarket boundary for proxy-wallet accounts. `buba-paint live` now supports a real authenticated `live_readonly` runtime that reuses the shared paper loop, persists real live sessions/account snapshots/reconciliation state, and keeps the shadow analysis pages useful via a paper track. Actual live order placement remains intentionally gated.

Settlements are applied only on authoritative Polymarket outcomes. Dynamic taker fees follow the March 30, 2026 crypto schedule by default (`feeRate=0.072`, `exponent=1`) while still supporting historical-by-date fee resolution and explicit overrides for sweeps. Reserve accounting is phase-aware: once a market closes, unresolved trades can move from active-market risk into pending-settlement reserve so the live bot can stop charging the strategy sleeve for closed risk while still keeping global capital locked until Gamma resolves the market. A shared agent monitors bot databases and exposes REST + WebSocket APIs. A dashboard (Rust backend + React frontend) provides a unified UI for status, trades, signals, and process control.

The reserve model and exact-run parity workflow are documented in [docs/pending-settlement-modes.md](./docs/pending-settlement-modes.md). Conservative pending-settlement handling is now the real default for new runs. Compatibility mode is still available, but only as a legacy/diagnostic preset.

Current release note: the latest point release hardens live websocket transport without changing trading semantics. All three feeds now share a bounded websocket connect timeout, Binance and CLOB reconnect if the socket stays open but no text market data arrives for too long, and CLOB reconnects coalesce stale resubscribe requests down to the newest market-token pair. The stale-data gates are unchanged, so the bot still blocks trading rather than trading blind.

For durable docs and future-work status, start with [docs/Readme.md](./docs/Readme.md). For live-money readiness details, use:

- [docs/live-trading-architecture.md](./docs/live-trading-architecture.md)
- [docs/polymarket-live-constraints.md](./docs/polymarket-live-constraints.md)
- [docs/live-session-runbook.md](./docs/live-session-runbook.md)

The current local tree is ready for authenticated readonly venue verification, replay-grade research capture, compact live-session telemetry, and pre-live review work. It is not ready for real-money deployment. `live_readonly` is a real venue/account monitor plus a shared shadow paper runtime. `live_trading` is still gated and cannot place orders.

Repository agent-instruction alias: [AGENTS.md](./AGENTS.md) points to the canonical [CLAUDE.md](./CLAUDE.md). Keep `CLAUDE.md` as the real source of truth.

## Quick Start

```bash
cargo build --release              # optimized binaries (paint, agent, dashboard)
cargo test                         # Rust test suites across all crates
cargo clippy -- -D warnings        # lint (zero warnings required)
make lint                          # fmt + clippy + strict Rust + TS comment audits
make docs-audit                    # docs, script, and derived-data hygiene audit

cd dashboard/client && npm install && npm test   # frontend Vitest suite
cd polymarket-sidecar && npm install && npm test # sidecar Vitest suite
cd dashboard/client && npm run dev               # dev server on :3000 (proxies to :3001)

# Or run the local paper dashboard stack via Docker:
docker compose up -d
```

Requires Rust 1.94+ (install via [rustup](https://rustup.rs)) and Node 22+ for the dashboard frontend. Docker Compose builds a local paper stack with paint, agent, and dashboard. It does not start the Polymarket sidecar or a `live_readonly` venue monitor.

## How It Works (paint bot)

Every 5 minutes, Polymarket opens a market: "Will BTC go Up or Down?" The paint bot now supports three strategy families, but the live portfolio router keeps them from competing on the same snapshot:

Latency arb: Binance spot price moves first, Polymarket sometimes lags. When Binance momentum exceeds the adaptive threshold and the relevant YES token is still cheap enough, the bot queues a taker-style buy with a simulated arrival delay. The fill model then checks the book at arrival time, enforces min size and tick size, and allows partial fills.

Spread capture: when UP ask + DOWN ask is cheap enough after fees, the bot queues two independent taker buys. The pair is not modeled as atomic. If only one leg fills, the residual directional position stays open and settles like any other trade.

Calm persistence: in quiet late-window regimes, the bot can buy the currently winning side if Binance stays on one side of the window open, realized volatility is low, recent open-crosses are rare, and Polymarket still offers positive expected edge after fees and slippage.

The portfolio router chooses one family per evaluation snapshot:

- `dislocation` -> latency-arb
- `structural_pair` -> spread-capture
- `calm` -> calm-persistence

Per-strategy capital sleeves and per-strategy trend filtering keep one family from starving another family by shared bankroll or shared suppression state.

For new live paper and `live_readonly` research runs, replay-grade raw feed events are written to `feed_events` and replayed by timestamp. The default storage profile keeps typed decision inputs, including compact Binance `bookTicker` rows, and drops bulky hot-path payload blobs so week-long paper runs remain practical. Signal-generation telemetry is persisted to `signal_metrics`, feed lifecycle events are persisted to `feed_health_events`, and no-signal strategy decisions are summarized into `strategy_rejection_summaries` so live diagnostics can explain why a strategy kept returning `None`. Live quote freshness now uses observed local receipt time when the CLOB source timestamp is missing or zero, while the raw `feed_events.event_at_ms` value is still preserved for replay/debugging truth. For older runs, the simulator falls back to synthetic `legacy_snapshot` events built from 1 Hz `tick_data`. That path is intentionally conservative and should be treated as lower-fidelity than new raw-event runs.

At window close, the bot records a provisional estimate for observability but waits for the authoritative Polymarket resolution before applying settlement to bankroll, Kelly state, trend tracking, or the circuit breaker. Closed trades can be reclassified from active-market risk into pending-settlement reserve immediately at window close. In conservative mode, that releases the strategy sleeve while keeping the full global reserve locked. In riskier mode, both the sleeve release and a configurable global-reserve haircut are available behind explicit config. Fees use Polymarket's dynamic formula: `fee = shares * price * feeRate * (price * (1 - price))^exponent`. The default live crypto params are `feeRate=0.072` and `exponent=1` as of March 30, 2026.

For operator defaults and exact env triples, do not rely on memory. Use [docs/pending-settlement-modes.md](./docs/pending-settlement-modes.md) and [.env.example](./.env.example).

### Three Feeds

- Binance market data (`wss://stream.binance.com:9443/stream?...`): combined `aggTrade`, `bookTicker`, and `depth@100ms` streams, with microsecond timestamps when supported.
- Polymarket CLOB (`wss://ws-subscriptions-clob.polymarket.com/ws/market`): order book snapshots + price changes, variable rate.
- Chainlink RTDS (`wss://ws-live-data.polymarket.com`): oracle BTC/USD price used as settlement reference, ~1 msg/s.

All feeds auto-reconnect with exponential backoff (1s base, 30s max, jitter). Websocket connects are bounded by `WEBSOCKET_CONNECT_TIMEOUT_MS`, Binance and CLOB have no-message watchdog reconnects, and Chainlink still detects silent staleness and force-reconnects. These transport safeguards reduce long stale-data gaps without relaxing the freshness rules that block trading on stale inputs.

## Architecture

The workspace contains three Rust crates, a React frontend, and a TypeScript Polymarket sidecar:

- `bots/paint` (`buba-paint`): the BTC Up/Down trading bot. Runs live trading, backtests, parameter sweeps, settlement verification (`verify-settlements`), and the `build-data` merge tool.
- `agent` (`buba-agent`): monitoring agent that sits alongside a bot, reads its SQLite database (WAL mode, read-only), and exposes status, trades, balance, chart-safe equity series, signals, grouped signals, stats, logs, trading summary, live-readiness details, and bot process control over REST. Polls the DB every 2 seconds and broadcasts changes over WebSocket.
- `dashboard/server` (`buba-dashboard`): dashboard backend. Manages users (Argon2 password hashing, JWT auth), proxies REST and WebSocket requests to one or more agents. Can serve the built React frontend as static files.
- `dashboard/client`: React/Vite frontend. The information architecture is split into `Monitor` and `Analysis`. `Overview` is the mixed triage page, `Execution` is the mode-aware account/execution cockpit, `Logs` is the operator event stream, and `Equity`, `Trades`, `Signals`, and `Strategies` are shadow-performance analysis pages. `/execution` is canonical. `/trading`, `/live`, and `/stats` remain compatibility redirects. Uses TanStack React Query for server state with WebSocket-driven cache invalidation. Installable as a PWA on iPhone, iPad, and Android with safe-area handling, responsive mobile layout (compact icon sidebar + drawer), dark mode (system/dark/light toggle, persisted), and trade notifications.
- `polymarket-sidecar`: authenticated Polymarket boundary for proxy-wallet accounts. Owns live preflight, account-state, and future order/redeem flows behind a local HTTP API. The current provider is real for readonly-safe surfaces (`/health`, `/account`, `/preflight`) and still stubbed for write paths. It does not place real orders yet.

### Data flow

The bot writes to SQLite. The agent reads that database (WAL mode, read-only) and exposes the data over REST + WebSocket. The dashboard backend proxies requests from the frontend to agents, authenticating with a shared secret. The React frontend polls via TanStack React Query and receives live updates over a proxied WebSocket connection that triggers cache invalidation.

Authentication happens at two layers: the frontend authenticates to the dashboard with username/password -> JWT (HS256, 24h expiry). The dashboard authenticates to agents with a shared secret (Bearer token from `dashboard.toml`).

### paint bot modules

Core loop: `cli.rs` (clap CLI parsing, command dispatch, including `live-preflight`), `live.rs` (shared runtime for `paper` and `live_readonly`, plus future `live_trading` dispatch), `live_readonly.rs` (readonly bootstrap and background venue monitor used by the shared runtime: preflight, live sessions, account snapshots, reconciliation, operator rollups), `config.rs` (all env-configurable settings, `set_param` for sweeps, execution mode, live budget caps, pending-settlement reserve knobs, and backtest settlement mode), `latency_probe.rs` (operator-facing endpoint/feed latency benchmark), `live_sidecar.rs` (typed HTTP client for the local Polymarket sidecar).

Strategies: `strategies/latency_arb.rs` (feature-scored stale-odds signal, adaptive threshold, cooldown), `strategies/spread_capture.rs` (fee-aware two-leg taker spread capture with legging-risk gates), `strategies/calm_persistence.rs` (late-window sign persistence in calm regimes), `signal_features.rs` (shared feature engine used by live paper and backtests).

Feeds: `feeds/binance_feed.rs` (combined Binance trade, top-of-book, and shallow-depth stream, with bounded connect timeout and no-message watchdog), `feeds/clob_feed.rs` (CLOB order book with incremental updates, best-bid-ask support, dynamic resubscription, and latest-token reconnect coalescing), `feeds/chainlink_feed.rs` (RTDS Chainlink prices + staleness detection), `feeds/util.rs` (exponential backoff with jitter, stable connection tracking, and structured disconnect metadata).

Data: `bankroll.rs` (per-strategy half-Kelly sizing, sleeves, caps, confidence curve, DD pause, and phase-aware active-vs-pending settlement reserve accounting), `position_manager.rs` (trade lifecycle, opposing position guard, authoritative settlement), `circuit_breaker.rs` (pause after consecutive losses), `tick_logger.rs` (1s telemetry sampling for dashboards and coarse inspection), `trend_tracker.rs` (strategy-scoped directional trend filter), `portfolio.rs` (regime router, family attribution, and non-competing portfolio helpers).

Execution: `executor.rs` (`ExecutionEngine`, shared by live paper trading and backtests, with simulated order latency, partial fills, no-fills, and execution metrics).

Shared orchestration: `strategy_cycle.rs` (shared live/backtest strategy evaluation, router blocking, trend suppression, spread affordability precheck, signal persistence decisions, and execution submission).

Fees and verification: `fees.rs` (historical fee schedule resolution plus Polymarket dynamic taker fee formula), `verify.rs` (backfill Polymarket resolutions from Gamma API, `verify-settlements` CLI command).

SDK integration: `polymarket.rs` (read-only wrapper around the official `polymarket-client-sdk` crate, queries CLOB API for market resolution status), `live_sidecar.rs` (readiness matrix, live preflight, account-state, and future live order/redeem request boundary).

Backtesting: `backtest/runner.rs` (core replay loop, including exact-run observed-resolution replay), `backtest/sweep.rs` (parallel parameter sweep via rayon, PID-based temp DBs, inherits env-backed config), `backtest/tick_replay.rs` (loads `feed_events` when available and falls back to `tick_data`), `backtest/window_manager.rs` (replays market windows from DB), `backtest/feed_state.rs` (simulated feed state), `backtest/momentum.rs` (rolling window momentum calculator).

Database: `db/database.rs` (rusqlite wrapper, prepared statements, WAL mode, bounded WAL settings, footprint reporting), `db/schema.rs` (additive schema migrations), `db/build_data.rs` (merges enriched run DBs into `market-data.db`, including signals and optional telemetry tables when present), `db/upgrade_history.rs` (in-place historical upgrade and metadata backfill for runs `004` through `009`).

Shared: `types.rs` (Signal, BookState, MarketWindow, TradeResult with settlement_status), `clock.rs` (Clock trait + SystemClock + BacktestClock), `errors.rs` (thiserror error types).

### Agent modules

`api.rs` (REST endpoints + WS route), `db_reader.rs` (read-only SQLite connection, status, trades, balance, chart-safe equity series, signals, grouped signals, stats, and live-detail queries), `ws.rs` (DB poller + WebSocket broadcast handler), `process_manager.rs` (ChildProcessManager for bot lifecycle control, NoopProcessManager for monitoring-only mode), `auth.rs` (shared-secret Bearer middleware), `types.rs` (BotStatus, TradeRow, EquitySeriesResponse, SignalGroupsResponse, WsMessage), `error.rs` (AgentError with HTTP status mapping).

### Dashboard server modules

`auth.rs` (Argon2 hashing, JWT creation/validation, auth middleware), `config.rs` (TOML config: server port, JWT secret, agents list), `db.rs` (SQLite users/sessions store), `proxy.rs` (HTTP proxy helpers for agent communication), `error.rs` (DashboardError with HTTP status mapping).

API routes: `api/auth_routes.rs` (login, me), `api/bots.rs` (list bots, proxy status, trades, balance, chart-safe equity series, signals, grouped signals, stats, logs, process controls, trading summary, and live detail routes), `api/users.rs` (admin-only user management), `api/ws_proxy.rs` (WebSocket proxy: validates JWT, bridges client <-> agent).

## Project Structure

```
buba-paint/
  Cargo.toml                       # workspace root (3 crates)
  CLAUDE.md                        # AI development guidelines
  Readme.md                        # this file
  docker-compose.yml               # full stack (paint + agent + dashboard)
  dashboard.toml                   # dashboard config (dev defaults for Docker)
  .env.example                     # bot environment variables template
  docs/                            # operator and parity docs
  bots/
    paint/                         # paint bot, BTC Up/Down 5m
      Cargo.toml
      Dockerfile
      src/
        main.rs, lib.rs, cli.rs, live.rs, config.rs, types.rs, ...
        feeds/                     # Binance, CLOB, Chainlink WebSocket feeds
        strategies/                # latency-arb, spread-capture, calm-persistence
        backtest/                  # tick replay, parameter sweep (rayon)
        db/                        # SQLite wrapper, schema, build-data
      tests/                       # integration tests (mock WS, wiremock)
  agent/                           # shared monitoring agent
    Cargo.toml
    Dockerfile
    src/
    tests/                         # integration tests
  dashboard/
    server/                        # dashboard backend (Rust/Axum)
      Cargo.toml
      src/
      tests/                       # integration tests
    client/                        # dashboard frontend (React/Vite)
      package.json
      src/
    Dockerfile                     # builds both server + client
  polymarket-sidecar/             # local TS sidecar for proxy-wallet auth/relayer
    package.json
    src/
  scripts/                         # audits, setup helpers, manual analysis scripts, archive
  data/                            # derived data (reproducible)
  runs/                            # primary live data (IRREPLACEABLE)
    001/ ... 012/                  #   DB, logs, analysis PNGs (LFS)
```

`runs/` contains primary data collected during live paper trading sessions over weeks. Do not edit these DBs manually. The only supported in-place mutation is `upgrade-history`, which performs additive schema upgrades and metadata backfills on historical runs. `data/` is derived and reproducible.

## CLI Reference

### Live paper trading (paint)

```bash
cargo run -p buba-paint --release -- live --db-path runs/009/buba-paint.db --balance 200
cargo run -p buba-paint --release -- live --set LATENCY_ARB_MAX_ASK=0.55

# Historical conservative run-018 parity baseline for reference only:
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false \
cargo run -p buba-paint --release -- live \
  --db-path runs/018/buba-paint.db \
  --balance 200 \
  --set LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008 \
  --set LATENCY_ARB_MAX_ASK=0.60 \
  --set LATENCY_ARB_MAX_POSITION_FRACTION=0.075 \
  --set SPREAD_CAPTURE_THRESHOLD=0.970

# Continue an existing live run after a bot-only robustness update:
WEBSOCKET_CONNECT_TIMEOUT_MS=10000 \
BINANCE_NO_MESSAGE_RECONNECT_MS=5000 \
CLOB_NO_MESSAGE_RECONNECT_MS=20000 \
cargo run -p buba-paint --release -- live \
  --db-path runs/011/buba-paint.db \
  --balance 200

# Live-readonly preflight through the local Polymarket sidecar.
# This validates proxy-wallet credentials, geoblock, user-stream readiness,
# and budget assumptions without enabling real order placement.
EXECUTION_MODE=live_readonly \
LIVE_SESSION_CASH_CAP_USD=100 \
LIVE_MAX_SINGLE_ORDER_USD=10 \
LIVE_MAX_OPEN_NOTIONAL_USD=25 \
LIVE_MAX_DAILY_LOSS_USD=15 \
LIVE_MAX_SESSION_DRAWDOWN_USD=20 \
LIVE_MIN_REQUIRED_CASH_USD=25 \
cargo run -p buba-paint --release -- live-preflight

# Run the real authenticated readonly venue runtime.
EXECUTION_MODE=live_readonly \
LIVE_SESSION_CASH_CAP_USD=100 \
LIVE_MAX_SINGLE_ORDER_USD=10 \
LIVE_MAX_OPEN_NOTIONAL_USD=25 \
LIVE_MAX_DAILY_LOSS_USD=15 \
LIVE_MAX_SESSION_DRAWDOWN_USD=20 \
LIVE_MIN_REQUIRED_CASH_USD=25 \
cargo run -p buba-paint --release -- live \
  --db-path runs/readonly/paint.db \
  --balance 100
```

`live-preflight` and `EXECUTION_MODE=live_readonly` now use the real readonly sidecar provider. They validate proxy-wallet credentials, host geoblock, clock drift, active-market metadata, account state, and authenticated user-stream connectivity without placing orders. In `live_readonly`, the shared runtime still evaluates strategies and produces shadow paper signals/trades/equity while the `Execution` page shows real venue/account truth. `EXECUTION_MODE=live_trading` is still rejected until the dedicated live trading runtime exists.

### Single backtest (paint)

```bash
cargo run -p buba-paint --release -- backtest \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 --set PEAK_DD_PAUSE_PCT=1.0

BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false \
cargo run -p buba-paint --release -- backtest \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 --end 2026-04-08T17:25 \
  --balance 200 \
  --set LATENCY_ARB_ENABLED=true --set SPREAD_CAPTURE_ENABLED=true --set CALM_PERSISTENCE_ENABLED=true
```

### Parameter sweep (paint)

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-15 --end 2026-03-31 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set TAKER_FEE_RATE=0.072 --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/example/sweep.csv

BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false \
cargo run -p buba-paint --release -- sweep \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 --end 2026-04-08T17:25 \
  --balance 200 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep LATENCY_ARB_MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.970 \
  --output data/sweeps/run-018-002/sweep.csv
```

`--sweep PARAM=start:end:step` generates a range; `--sweep PARAM=a,b,c` enumerates. `--set PARAM=value` fixes a parameter without sweeping. Boolean env vars and boolean `--set` overrides accept `true/false`, `1/0`, `yes/no`, and `on/off`; prefer `true/false` in docs and operator commands. Backtests and sweeps now start from `Config::from_env()`, so env-backed knobs such as `BACKTEST_SETTLEMENT_MODE` and the pending-settlement reserve settings apply to both commands unless a CLI `--set` overrides them.

The recommended exact-run workflow is:

- keep `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`
- keep pending-settlement reserve handling at the default conservative mode
- override the reserve knobs only if you are intentionally comparing against compatibility or risky mode

The current calm point-release candidate after the `run-011` forensic replay and `calm-004` confirmation is:

- `CALM_PERSISTENCE_MAX_ASK=0.65`
- `CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.05`
- keep the rest of the calm row unchanged (`30-90s`, `6bps`, `distance/vol=1.0`, `alignment=0.5`, `fairBias=0.35`, `realizedVol15s<=80`, `openCrosses30s<=1`, `quoteChurn<=100`, `sleeve=0.05`)

### Verify settlements (paint)

```bash
cargo run -p buba-paint --release -- verify-settlements --db data/market-data.db --concurrency 15
```

Fetches actual Polymarket resolutions from the Gamma API for all markets in the database and compares them against Chainlink-derived settlements. Reports accuracy rate and stores the authoritative outcomes.

### Initialize database (paint)

```bash
cargo run -p buba-paint --release -- init-db --db-path runs/009/buba-paint.db --balance 200
```

Creates an empty database with all tables and an initial balance event. Use this to prepare a DB for the agent before starting the bot. Avoids the need to start and kill the bot just to create the file.

### Build merged market data (paint)

```bash
cargo run -p buba-paint --release -- build-data
cargo run -p buba-paint --release -- build-data --runs-dir runs --output data/market-data.db
```

Merges tick data, markets, and trade results from all run DBs into a single database for backtesting. Source DBs opened read-only. Idempotent.

### Upgrade historical run DBs in place (paint)

```bash
cargo run -p buba-paint --release -- upgrade-history
cargo run -p buba-paint --release -- upgrade-history \
  --runs-dir runs --from-run 4 --to-run 9 \
  --rebuild-derived --output data/market-data.db
```

Adds the latest schema, backfills market metadata and authoritative outcomes into run DBs, synthesizes `feed_events` from legacy `tick_data`, and optionally rebuilds `data/market-data.db`. HTTP payloads are cached under `data/backfill-cache/`.

### Probe endpoint and feed latency (paint)

```bash
cargo run -p buba-paint --release -- latency-probe
cargo run -p buba-paint --release -- latency-probe --timeout-ms 3000
```

Measures the current host's request or websocket-connect timing to Gamma, Binance, RTDS, and the Polymarket CLOB. For websocket feeds it also waits for the first data message and reports message age when the payload exposes a source timestamp.

### Inspect live DB footprint (paint)

```bash
cargo run -p buba-paint --release -- db-footprint --db-path /tmp/paint.db
```

Prints on-disk DB and WAL sizes plus grouped `feed_events` counts by source and event type. Use this before long paper runs if storage growth looks suspicious.

Example no-signal inspection query:

```sql
SELECT timestamp_ms, market_id, strategy, reason, count, details_json
FROM strategy_rejection_summaries
ORDER BY timestamp_ms DESC
LIMIT 20;
```

If the normal bot log only needs the operator view, look for `strategy rejection rollup` lines. Those rollups keep the top reasons and mean quote/freshness context readable in the log, while the full structured detail stays in `strategy_rejection_summaries`.

Example execution inspection query:

```sql
SELECT signal_id,
       decision_status,
       rejection_reason,
       order_submitted_at_ms,
       expected_arrival_at_ms,
       order_processed_at_ms,
       effective_arrival_delay_ms
FROM signal_metrics
ORDER BY signal_id DESC
LIMIT 20;
```

Normal operator logs now also emit `paper order filled`, `paper order missed`, and `paper execution rollup` lines so queued-order outcomes are readable without opening SQLite.

## Configuration

All settings via environment variables or `--set` CLI flag.

- `FEED_EVENT_STORAGE_PROFILE=replay_grade|compact|full_debug`
  - `replay_grade` is the default for research runs. It keeps typed replay fields, including compact Binance book-ticker rows, and drops bulky hot-path payload blobs.
  - `compact` is descriptive-only. It suppresses high-rate Binance book-ticker persistence and is blocked by the sweep preflight gate.
  - `full_debug` keeps raw payload retention for short local diagnostics and should not be the week-long live-paper default.

Core: `DB_PATH` (default `./data/paint.db`) is the SQLite database path. `LOG_LEVEL` (default `info`): debug, info, warn, error. `TICK_INTERVAL` (default `1000`): coarse telemetry sampling interval in ms. `GAMMA_POLL_INTERVAL` (default `60000`): Gamma API poll interval in ms. `CHAINLINK_STALE_MS` (default `30000`): force-reconnect after silence. `WEBSOCKET_CONNECT_TIMEOUT_MS` (default `10000`): bound websocket handshake/connect hangs before normal reconnect backoff resumes. `BINANCE_NO_MESSAGE_RECONNECT_MS` (default `5000`) and `CLOB_NO_MESSAGE_RECONNECT_MS` (default `20000`): force reconnect when the socket stays open but no text market data arrives. `MAX_SIGNAL_FEED_AGE_MS` and `MAX_QUOTE_AGE_MS` cap stale inputs for signal generation.

Strategy toggles and routing: `LATENCY_ARB_ENABLED`, `SPREAD_CAPTURE_ENABLED`, and `CALM_PERSISTENCE_ENABLED` enable the three families. `REGIME_DETECTION_ENABLED` turns on the portfolio router. `TREND_FILTER_PER_STRATEGY` scopes trend suppression to the strategy family instead of one shared global state.

Binance feed: `BINANCE_TRADE_STREAM`, `BINANCE_BOOK_TICKER_STREAM`, and `BINANCE_DEPTH_STREAM` control the combined Binance market-data subscription. `BINANCE_WS_URL` remains as a backward-compatible override for older aggTrade-only setups.

Latency arb: `LATENCY_ARB_MOMENTUM_THRESHOLD` (default `0.0015`) is the base momentum fraction (0.15%). `LATENCY_ARB_ADAPTIVE_WINDOW_MS` (default `1800000`) is the rolling time window used by the adaptive threshold. `LATENCY_ARB_MAX_ASK` (default `0.55`): max ask to consider stale. `LATENCY_ARB_MIN_ASK` (default `0.30`): min ask, rejects cheap tokens. `LATENCY_ARB_COOLDOWN_MS` (default `60000`): cooldown between signals. `MOMENTUM_WINDOW_MS` (default `30000`): momentum rolling window. `LATENCY_ARB_MAX_POSITION_FRACTION` is an optional latency-arb-only sleeve. If it is unset, latency-arb falls back to `MAX_POSITION_FRACTION`.

Spread capture: `SPREAD_CAPTURE_THRESHOLD` (default `0.998`) is the hard outer cap on UP+DOWN ask sum. The strategy also requires positive projected net edge after fees and simulated fills. `SPREAD_CAPTURE_MIN_ASK` (default `0.15`): reject degenerate books. `SPREAD_CAPTURE_MAX_LEG_SKEW_MS` (default `25`): require the UP and DOWN books used for one spread decision to be near-synchronous. Mixed-time books are rejected as `legs_out_of_sync`. `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S` (default `8`) caps the observed top-of-book churn before the strategy treats the legging risk as too high.

Calm persistence: `CALM_PERSISTENCE_MIN_WINDOW_TIME_MS` and `CALM_PERSISTENCE_MAX_WINDOW_TIME_MS` bound the late-window entry slice. `CALM_PERSISTENCE_MAX_ASK` is the calm-family YES-ask cap and is now used consistently in both signal generation and the shared single-order execution path. `CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS` and `CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD` require the current sign versus the window open to be large enough relative to realized volatility. `CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS`, `CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S`, and `CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S` keep the strategy in quiet windows. `CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION` requires enough recent microstructure agreement, `CALM_PERSISTENCE_MAX_FAIR_BIAS` caps the internal fair-value adjustment, `CALM_PERSISTENCE_MIN_EXPECTED_EDGE` is the calm-only post-fee/slippage quality floor, and `CALM_PERSISTENCE_MAX_POSITION_FRACTION` is the calm-only capital sleeve. Calm duplicate pending/open-position attempts are now rejected before signal persistence and recorded through `strategy_rejection_summaries` instead of bloating `signals` / `signal_metrics`.

Bankroll: `STARTING_BALANCE` (default `150`) is the initial paper balance in USD. `MAX_POSITION_FRACTION` (default `0.10`): max fraction per trade (10%). `MAX_POSITION_USD_FRACTION` (default `0.20`): hard cap per trade (20%). `MAX_POSITION_USD` (default `500`): absolute hard cap in USD regardless of balance. `MIN_BALANCE_THRESHOLD` (default `20`): stop trading below this. `MAX_DRAWDOWN_PCT` (default `0.50`): stop at 50% drawdown.

Kelly criterion: `KELLY_FRACTION` (default `0.5`) is the half-Kelly multiplier. `MIN_WIN_RATE_FOR_KELLY` (default `0.52`): min win rate to apply Kelly. `MIN_TRADES_FOR_KELLY` (default `20`): fixed fraction until enough trades. `KELLY_ROLLING_WINDOW` (default `30`): rolling window per strategy. `MIN_KELLY_FLOOR` (default `0.03`): min fraction floor (3%). `MIN_BET_USD` (default `5`): min bet size in USD.

Fees: `TAKER_FEE_RATE` (default `0.072`) and `TAKER_FEE_EXPONENT` (default `1`) are the live crypto defaults as of March 30, 2026. If not explicitly overridden, the engine can still resolve historical fee params by market end timestamp when replaying older periods.

Execution: `EXECUTION_MODE` (default `paper`): `paper`, `live_readonly`, or `live_trading`. `paper` and `live_readonly` both use the shared execution engine for shadow execution. `live_trading` is still intentionally gated and does not place real orders yet. `SIM_ORDER_LATENCY_MS` (default `250`) controls simulated order-arrival delay. It is a paper-trading heuristic, not a measured venue latency, so later calibration should use the persisted effective-arrival telemetry rather than treating `250` as ground truth. `MAX_BOOK_STALENESS_MS` (default `1500`) rejects fills against stale books. Submit-time sizing now checks both the market minimum size and `MIN_BET_USD` before an order is queued, so obviously too-small orders fail early as `below_market_min_size_on_submit` or `below_min_bet_on_submit` instead of turning into guaranteed misses later.

Strategy sleeves: `SPREAD_CAPTURE_MAX_POSITION_FRACTION`, `CALM_PERSISTENCE_MAX_POSITION_FRACTION`, and `LATENCY_ARB_MAX_POSITION_FRACTION` are optional strategy-only balance caps. If any of them is unset, that family falls back to `MAX_POSITION_FRACTION`. The shared hard caps `MAX_POSITION_USD_FRACTION` and `MAX_POSITION_USD` still apply.

Position limits and safety: `MAX_OPEN_POSITIONS` (default `5`): max concurrent positions. `MIN_WINDOW_TIME_MS` (default `90000`): don't enter with <90s left. `CIRCUIT_BREAKER_LOSSES` (default `3`): pause after N consecutive losses. `CIRCUIT_BREAKER_PAUSE_MS` (default `900000`): pause duration (15 min). `PEAK_DD_PAUSE_PCT` (default `0.30`): pause at 30% drawdown from peak. `PEAK_DD_PAUSE_MS` (default `3600000`): DD pause duration (1 hour). `DD_PAUSE_RECOVERY_PCT` (default `0.05`): DD must recover by 5% before re-arming. `RECONNECT_MIN_STABLE_MS` (default `5000`): min connection duration to reset backoff. `RECONNECT_MAX_FAILURES` (default `20`): feed circuit breaker threshold. `RECONNECT_PAUSE_MS` (default `300000`): feed circuit breaker pause (5 min). Healthy-feed strategy behavior is unchanged by these transport knobs. They only bound reconnect hangs and shorten stale-data downtime.

Resolution polling: `RESOLUTION_INITIAL_DELAY_MS` (default `30000`): how long to wait after nominal close before the first authoritative Gamma poll. `RESOLUTION_POLL_DELAY_MS` (default `10000`): delay between later reconciliation attempts for any market that still has open trades and is not yet resolved. `RESOLUTION_POLL_RETRIES` is retained for helper/test code, but the live bot now keeps retrying unresolved open-trade markets until Gamma resolves them, including after a restart over the same DB.

Trend filter (experimental, off by default): `TREND_FILTER_ENABLED` (default `false`): enable counter-trend suppression. `TREND_FILTER_THRESHOLD` (default `0.30`): bias threshold to suppress. `TREND_FILTER_WINDOW` (default `10`): recent outcomes to consider. `TREND_FILTER_PER_STRATEGY` scopes the state by strategy family so one family does not suppress another family.

## Database Schema

SQLite (WAL mode). Python scripts can read concurrently while the bot writes.

- tick_data: 1-second sampled prices from all feeds. This remains available for dashboards and coarse inspection. Columns: timestamp (Unix ms), source (binance/clob_up/clob_down/chainlink), price, bid, ask, bid_size, ask_size.
- run_metadata: key-value run metadata such as feed storage profile, replay-quality class, and observed feed-event classes.
- feed_events: canonical replay source for the live-like simulator. Stores raw or synthesized event timing with normalized book fields and a fidelity marker (`raw_event` or `legacy_snapshot`).
- markets: one row per 5-minute window. Columns: market_id (Gamma API ID), question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status (active/closed/resolved), outcome (authoritative runtime outcome), polymarket_outcome, resolution_source, fee_profile, min-size and tick-size metadata, and reward metadata.
- signals: every strategy detection event. Includes the market link, replay fidelity, and execution timing fields when available.
- signal_metrics: per-signal telemetry. Stores generation, submission, expected-arrival timing, actual order-processing timing, effective arrival delay, per-feed ages, expected fee/slippage/edge, and a JSON feature snapshot. The snapshot now also records the per-leg effective book timestamps, `interLegSkewMs`, and the actual ask pair the strategy saw. `decision_status` progresses through queue and execution outcomes, and `rejection_reason` is reused for pre-submit queue rejections (`duplicate_pending_order`, `duplicate_open_position`, `max_open_positions`, `below_market_min_size_on_submit`, `below_min_bet_on_submit`) and post-submission miss reasons such as stale books or zero liquidity on arrival. Calm duplicate pending/open-position blocks are handled earlier and appear in `strategy_rejection_summaries` instead of generating redundant signal rows.
- feed_health_events: feed lifecycle telemetry. Stores connect, disconnect, stale, reconnect, and resubscribe events with optional market context. `details_json` now carries structured reconnect metadata such as `causeClass`, `attempt`, `reconnectDelayMs`, `connectionLifetimeMs`, `afterResubscribe`, and timeout/error context when available.
- strategy_rejection_summaries: aggregated no-signal diagnostics. Stores the strategy, rejection reason, count, and compact JSON summaries of representative quote, freshness, edge, and spread-leg skew values. The human log mirrors these as concise rollups instead of dumping the full JSON blob.
- simulated_trades: opened positions. Links to market and signal, tracks requested size/price, filled size/average fill, fill status, execution group id for spread pairs, execution mode, and optional live order ids.
- trade_results: settlement P&L. Links to trade, records settlement price, gross/net PnL fields, fee_amount, and settlement status.
- balance_log: bankroll history. Records every balance change with timestamp, event type (init, trade_close, settlement_correction), trade reference, amount, and running balance.
- settlement_audit: tracks prediction accuracy per trade (added v0.7). Columns: trade_id, market_id, our_prediction, polymarket_outcome, matched, timestamp.

See `bots/paint/src/db/schema.rs` for full DDL. New columns are added via `add_column_if_missing` for backward compatibility with old run databases.

## Backtesting

The backtester replays historical data through the real strategy code, the shared strategy-cycle orchestration, and the shared execution engine. When `feed_events` exist, raw events replay at their recorded timestamps. When they do not, the backtester synthesizes `legacy_snapshot` replay from 1 Hz `tick_data` and uses the first snapshot at or after simulated order arrival as a conservative proxy. Dynamic fees are applied, order-arrival latency is modeled, and spread trades can leg into residual positions.

For exact pulled-run calibration, the preferred mode is `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`. In that mode, replay keeps trades in pending settlement after market close until the observed authoritative resolution timestamp recorded in the pulled live run, then applies the same phase-aware reserve release rules as live. That is the closest available simulation when judging whether a candidate parameter set would have survived real sleeve pressure on a specific live run.

Old runs are still limited by the 1 Hz archive. The new simulator improves execution realism materially, but it cannot recreate queue position, quote lifetime, or sub-second book changes that were never recorded.

### Sweep Results

- 001 (TS): ~40 min, best PnL $14,602. INVALID, stale temp DBs inflated balance.
- 002 (TS): ~40 min, best PnL $1,282. Peak DD pause throttled at $200 start.
- 003 (TS): ~40 min, best PnL $4,070. Baseline, DD pause disabled, matches live.
- rust-004: 42s, best PnL $4,070. Rust parity validation (identical to 003).
- rust-005: 42s, best PnL $4,070. Post-refactor, identical to rust-004.
- rust-006: 4 min, best PnL $815,678. Full range (Feb 15 to Mar 20, 8.8M ticks). No liquidity or fee constraints.
- rust-007: 3 min, best PnL $4.7T. Extended range with run 008 (Feb 15 to Mar 26, 11M ticks). No liquidity or fee constraints. Fantasy numbers.
- rust-008: 3 min, best PnL $88k. Same data as rust-007 with liquidity clamping + dynamic fees. First realistic sweep.
- rust-009: 3 min, identical to rust-008. This was the last sweep on the old snapshot simulator before authoritative Polymarket outcomes and the live-like execution rewrite.

Pre-live-like sweeps often fixed `PEAK_DD_PAUSE_PCT=1.0` and an artificially low spread threshold to suppress 1 Hz snapshot artifacts. New sweeps should document their fee overrides, latency assumptions, and spread threshold handling explicitly in the run note.

## Run History

- Run 001: v0.0, first test run.
- Run 002: 5h, 9 trades, 55.6% WR, +$69. v0.1, fixed 100-token bets.
- Run 003: 1h, 1 trade, 0% WR, -$11. v0.2, bankroll-aware sizing.
- Run 004: 96h, 76 trades, 51.3% WR, +$719, $200->$919, peak $1,556. v0.2.
- Run 005: 25h, 11 trades, 36.4% WR, -$5. v0.3, over-filtering bug.
- Run 006: 267h, 292 trades, 56.5% WR, +$4,565, $200->$4,765, peak $9,678. v0.4.
- Run 007: 222h, 187 trades, 56.7% WR, +$5,488, $200->$5,688, peak $8,130. v0.5 TS.
- Run 008: 159h, 465 trades, 74.2% reported WR, $200->$1,415,928 (paper). v0.6 Rust, deployed Mar 20. Settlement used Binance (not Polymarket oracle). Verification against Polymarket showed real WR of 50.5%. The reported PnL is not trustworthy. Parameters: `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0012`, `LATENCY_ARB_MAX_ASK=0.60`, `MAX_POSITION_FRACTION=0.05`, `SPREAD_CAPTURE_THRESHOLD=0.50`.
- Run 009 (first attempt, v0.7): deployed Mar 28, failed. Gamma resolution polling too short (10s window, resolution takes 40s-4min). All settlements fell back to Binance. Discarded.
- Run 009 (second attempt, v0.8): deployed Mar 28, ran 9.8h, 7 trades. Two critical bugs: (1) market_discovery set current_window to next_slot 3-5 min early, causing 6/7 trades to be assigned to the wrong market window; (2) deferred resolution polling too short, all 7 trades stuck in provisional status. Corrected 57.1% real WR (4 wins, 3 losses), +$2.17. Both bugs fixed in v0.8.1.
- Run 009 (v0.8.1): deployed Mar 29. Fixes: window activation delayed until start_time, resolution polling extended to 5 min, debug logging on all trade rejections, init-db CLI, open_price fallback.

## Testing

Test inventory changes frequently as the workspace evolves. Use `cargo test`, `make test-all`, and the frontend test commands below as the authoritative source of truth instead of relying on hard-coded counts.

- paint bot: broad Rust unit and integration coverage across replay, execution, fees, accounting, migrations, feeds, strategy logic, and mocked live-system scenarios. The slow live-system lane includes circuit-breaker recovery within the same bot run.
- agent: Rust tests cover REST endpoints, WS polling/broadcast, DB reader compatibility across legacy/new schemas, process manager behavior, and auth middleware.
- dashboard server: Rust tests cover auth/JWT, bot proxy handlers, WS proxy, config parsing, error mapping, and integration proxy flows against a mock agent.
- dashboard client: Vitest covers API, WS, auth store, hooks, components, and pages. Network-bound tests support MSW. Browser E2E uses Playwright.
- Coverage gates now measure Rust library and integration-test coverage while explicitly excluding thin `main.rs` bootstrapping entrypoints. Floors are currently `80%` for `buba-paint`, `90%` for `buba-agent`, `90%` for `buba-dashboard`, and `80%` for the frontend.
- Comment policy: `tools/rust-comment-policy/` enforces concise rustdoc on every Rust function and rejects non-doc Rust comments. `scripts/ts_comment_audit.mjs` rejects non-directive comments in the frontend TypeScript code. `make lint` runs both checks, and `make comment-audit` prints the current Rust and TS/TSX audit summaries.

```bash
cargo test                              # all Rust test suites
cargo clippy --workspace -- -D warnings # lint
cargo fmt --all --check                 # format check
make lint                              # fmt + clippy + strict Rust + TS comment audits
make comment-audit                     # detailed Rust rustdoc/comment backlog report
make test-fast                          # workspace Rust libs + frontend Vitest
make test-integration                   # stable Rust integration suites
make test-slow                          # bot live-system suite
make test-e2e                           # Playwright browser tests
make test-all                           # fast + integration + slow + browser E2E
make coverage                           # Rust coverage summaries + frontend coverage
make coverage-gate                      # component coverage regression floors
cd dashboard/client && npm test         # frontend Vitest suite
cd dashboard/client && npm run test:e2e # Playwright browser E2E
cd dashboard/client && npm run test:coverage
```

TDD is strictly enforced. When a test fails, fix the code, not the test.

Rust comment policy is now explicit. Every Rust function should carry concise `///` rustdoc, including private helpers and tests. Inline comments inside Rust function bodies are treated as backlog and should be removed by rewriting the code so the structure and names carry the meaning directly. Whenever a file is touched, existing comments must be checked for staleness and either updated or deleted.

## Deployment

The full stack requires three processes: bot, agent, and dashboard.

```bash
# 1. Build
cargo build --release
cd dashboard/client && npm run build  # produces dist/ for static serving

# Seed the bot DB once
./target/release/buba-paint init-db --db-path runs/010/buba-paint.db --balance 200

# 2. Start the agent (manages bot lifecycle)
AGENT_SECRET=your-secret ./target/release/buba-agent \
  --db-path runs/010/buba-paint.db \
  --port 9090 \
  --bot-cmd "./target/release/buba-paint live --db-path runs/010/buba-paint.db --balance 200 --set LATENCY_ARB_ENABLED=true --set SPREAD_CAPTURE_ENABLED=true --set CALM_PERSISTENCE_ENABLED=true --set REGIME_DETECTION_ENABLED=true --set TREND_FILTER_PER_STRATEGY=true --set LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008 --set LATENCY_ARB_MAX_ASK=0.65 --set LATENCY_ARB_MAX_POSITION_FRACTION=0.05 --set MAX_POSITION_FRACTION=0.05 --set SPREAD_CAPTURE_THRESHOLD=0.970 --set SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05 --set CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000 --set CALM_PERSISTENCE_MAX_WINDOW_TIME_MS=90000 --set CALM_PERSISTENCE_MAX_ASK=0.65 --set CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6 --set CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0 --set CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.5 --set CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35 --set CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.05 --set CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=80 --set CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=1 --set CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=100 --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 --set TAKER_FEE_RATE=0.072 --set TAKER_FEE_EXPONENT=1 --set SIM_ORDER_LATENCY_MS=250"

# 3. Start the dashboard (serves frontend + proxies to agent)
ADMIN_USER=admin ADMIN_PASSWORD=changeme JWT_SECRET=your-jwt-secret \
  ./target/release/buba-dashboard \
  --config dashboard.toml \
  --static-dir dashboard/client/dist \
  --port 3001

# 4. Start the bot from the dashboard UI, or directly:
curl -X POST -H "Authorization: Bearer your-secret" http://localhost:9090/api/bot/start
```

`AGENT_SECRET` is mandatory in production-style startup. The agent now exits nonzero if it is missing or blank instead of silently accepting an empty secret.

The dashboard frontend builds with a Node 22+ toolchain. If the production host has an older Node version, build `dashboard/client/dist` locally and deploy the static bundle.

For Docker local testing: `docker compose up` starts the paper-only stack: paint bot, agent, and dashboard. It does not start the Polymarket sidecar or authenticated `live_readonly` monitoring. The bot creates its own DB on first run; no seeding step is needed. The dashboard is at `http://localhost:3001` (admin/changeme). For frontend hot reload during development, run `cd dashboard/client && npm run dev` against the Docker backend on port 3001. The `dashboard.toml` at the project root has dev defaults that match the compose env vars. Override secrets via `.env` (see `.env.example`).

The paint bot runs indefinitely, rolling through 5-minute windows. Ctrl+C for graceful shutdown (final stats printed, feeds disconnect, DB closes).

If the bot is redeployed mid-window, it now recovers the earliest persisted Binance tick inside that market window before falling back to the current live Binance price. That keeps the window-open drift calculation stable across restarts on the same DB.

When the live CLOB feed omits a usable source timestamp, the bot now treats the local observed receipt time as the freshness clock for trading decisions. Freshness for a binary market now reflects the older of the two book sides, so one fresh side cannot mask one stale side while the raw source timestamps remain preserved in `feed_events`.

### Preferred `buba-paint` workflow

The production host is `buba-paint`. Treat it as a simple release directory plus runtime directory layout, not as a place to edit code ad hoc.

Remote layout:

- releases live under `~/buba-paint-live/releases/<timestamp>`
- `~/buba-paint-live/current` is a symlink to the active release
- active runtime state lives under `~/buba-paint-live/runtime/run-0NN`
- disposable backups live under `~/buba-paint-live/runtime/backups`
- archived old runs live under `~/buba-paint-live/runtime/archive`

Preferred staging flow:

1. Finish code, docs, and tests locally first.
2. Run the local gates you actually trust: `make lint`, `make test-all`, `make coverage-gate`, `cargo build --release`.
3. Build the frontend locally with `cd dashboard/client && npm run build`.
4. Stage to a fresh remote release directory with `rsync`. Exclude `.git`, `target`, `data`, `runs`, `dashboard/client/node_modules`, and the old `dashboard/client/dist`.
5. Copy the already-built local `dashboard/client/dist` into the fresh release directory.
6. Build the Rust binaries on the server from that fresh release directory.

The server still runs a Node 18 toolchain. Do not rely on building the frontend there. Treat the locally-built `dashboard/client/dist` bundle as the deployable artifact until the server toolchain is upgraded.

The repository now carries operations templates under `ops/`. Use `ops/Readme.md` and the systemd user-service templates for sidecar, bot, agent, and dashboard instead of improvising `nohup` restarts.

`buba-paint` process model:

- the sidecar runs under supervision from `~/buba-paint-live/current/polymarket-sidecar`
- the sidecar env lives at `~/buba-paint-live/config/sidecar.env`
- the sidecar log lives at `~/buba-paint-live/logs/sidecar.log`
- the bot is started directly, usually through `script -qefa` so its ANSI log stream lands in the run log cleanly
- the agent runs in `--monitor-only` mode and reads the bot DB
- the dashboard serves the static frontend bundle from `~/buba-paint-live/current/dashboard/client/dist` and proxies to the monitor-only agent

Preferred fresh-run deploy:

1. Stop sidecar, bot, agent, and dashboard.
2. Verify there are no stale processes left before switching releases. Check both the wrapper and the child processes:

```bash
ssh buba-paint 'ps -eo pid=,args= | awk "/script -qefa|buba-paint live|buba-agent|buba-dashboard/ && !/awk/ && !/bash -c/ {print}"'
```

3. Archive or discard the old runtime according to the experiment plan.
4. Create a fresh `runtime/run-0NN` directory with a fresh DB and log.
5. Point `current` at the new release.
6. Start bot, then agent, then dashboard.

Preferred partial update over an existing run:

Use this only for code-only fixes where the run should remain comparable, for example diagnostics, logging, dashboard/agent fixes, restart-safe bot fixes, or live-feed transport hardening that leaves strategy semantics unchanged. Do not use it for strategy or parameter changes that should start a fresh experiment.

1. Back up the current run DB and log into `runtime/backups`.
2. Stop sidecar, bot, agent, and dashboard.
3. Verify no stale processes remain from the old release path and no old `buba-paint live` child is still attached to the run DB.
4. Point `current` at the new release.
5. Restart the supervised sidecar first, then restart over the same runtime directory, DB, and log. If the release added new live robustness knobs such as websocket timeouts or no-message watchdogs, set them explicitly in the sidecar and bot environments instead of relying on memory.
6. Verify that the bot recovered the same active window correctly and continued the run.

Minimum remote acceptance checks:

- `readlink -f ~/buba-paint-live/current` points to the intended release
- `curl http://127.0.0.1:3210/health` returns `ok: true` and a sane readiness payload
- `curl http://127.0.0.1:9090/health` returns `{"ok":true}`
- `curl http://127.0.0.1:3000/health` returns `{"ok":true}`
- `sqlite3 ~/buba-paint-live/runtime/run-0NN/paint.db "pragma quick_check;"` returns `ok`
- `ps -eo pid=,args=` shows only the new release path
- the bot log shows sane startup and expected strategy rollups

Disk cleanup policy:

- if space gets tight, prune old releases, archived runs, and disposable backups on `buba-paint`
- do not mass-delete remote history by default if space is still comfortable
- never delete local `runs/` or local `data/` as part of server cleanup. Local historical data is the canonical research asset

## Analysis Scripts

```bash
python3 scripts/analysis/chart-run.py              runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/pnl_curve.py              runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/latency_distribution.py   runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/spread_over_time.py       runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/signal_frequency.py       runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/binance_vs_chainlink.py   runs/012/server-20260424-183503/paint.db
```

Requires Python 3 with matplotlib, pandas, numpy. Each script produces a `.png`; they use a non-interactive matplotlib backend.

These helpers require an explicit DB path and are legacy-compatible charting tools. Use replay-quality validation and backtester reports for parameter sweeps; do not use these quick charts as sweep-grade evidence.

## Cross-compilation

- Dev: macOS aarch64 (Apple Silicon)
- Prod: Linux aarch64 (AWS t4g.small, Ubuntu 24.04)
