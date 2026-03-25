# buba

Paper-trading platform for Polymarket prediction markets. paint is the first bot: a 5-minute BTC Up/Down strategy connecting to three live WebSocket feeds, detecting latency-arbitrage and spread-capture opportunities, simulating trades with bankroll-aware position sizing (per-strategy half-Kelly criterion), and logging everything to SQLite. A shared agent monitors any bot's database and exposes a REST + WebSocket API. A dashboard (Rust backend + React frontend) provides a unified UI for all bots. Built in Rust for low-latency execution and fast backtesting (275-combo parameter sweep in ~42 seconds via rayon parallelism). 874 tests across the workspace.

No real orders, no wallet, no private keys. This is a data-collection and strategy-validation tool.

## Quick Start

```bash
cargo build --release              # optimized binaries (paint, agent, dashboard)
cargo test                         # 743 Rust tests across all crates
cargo clippy -- -D warnings        # lint (zero warnings required)

cd dashboard/client && npm install && npm test   # 131 frontend tests
cd dashboard/client && npm run dev               # dev server on :3000 (proxies to :3001)

# Or run the full stack via Docker:
docker compose up -d
```

Requires Rust 1.85+ (install via [rustup](https://rustup.rs)) and Node 22+ for the dashboard frontend.

## How It Works (paint bot)

Every 5 minutes, Polymarket opens a market: "Will BTC go Up or Down?" The paint bot exploits two edges:

Latency arb: Binance spot price moves faster than the Polymarket CLOB reprices. When Binance shows strong momentum but CLOB odds are stale, the bot logs a simulated directional buy. Features adaptive momentum threshold, 60s cooldown, min/max ask filters, and confidence scaling.

Spread capture: when UP ask + DOWN ask < $1.00, the market is mispriced. The bot buys both sides for guaranteed profit at settlement. Rejects entries where either side is below $0.15.

At window close, positions settle binary: winning side pays $1, losing pays $0. P&L computed under four fee assumptions (0-3%).

### Three Feeds

- Binance aggTrade (`wss://stream.binance.com:9443/ws/btcusdt@aggTrade`): per-trade BTC/USDT price, ~20-100 msg/s.
- Polymarket CLOB (`wss://ws-subscriptions-clob.polymarket.com/ws/market`): order book snapshots + price changes, variable rate.
- Chainlink RTDS (`wss://ws-live-data.polymarket.com`): oracle BTC/USD price used as settlement reference, ~1 msg/s.

All feeds auto-reconnect with exponential backoff (1s base, 30s max, jitter). Chainlink detects silent staleness and force-reconnects.

## Architecture

The workspace contains three Rust crates and a React frontend:

- `bots/paint` (`buba-paint`): the BTC Up/Down trading bot. Runs live trading, backtests, parameter sweeps, and the `build-data` merge tool.
- `agent` (`buba-agent`): monitoring agent that sits alongside a bot, reads its SQLite database (WAL mode, read-only), and exposes status, trades, balance, signals, stats, logs, and bot process control over REST. Polls the DB every 2 seconds and broadcasts changes over WebSocket.
- `dashboard/server` (`buba-dashboard`): dashboard backend. Manages users (Argon2 password hashing, JWT auth), proxies REST and WebSocket requests to one or more agents. Can serve the built React frontend as static files.
- `dashboard/client`: React/Vite frontend. Displays bot status, equity curves, trades, signals, and logs. Uses TanStack React Query for server state with WebSocket-driven cache invalidation.

### Data flow

The bot writes to SQLite. The agent reads that database (WAL mode, read-only) and exposes the data over REST + WebSocket. The dashboard backend proxies requests from the frontend to agents, authenticating with a shared secret. The React frontend polls via TanStack React Query and receives live updates over a proxied WebSocket connection that triggers cache invalidation.

Authentication happens at two layers: the frontend authenticates to the dashboard with username/password -> JWT (HS256, 24h expiry). The dashboard authenticates to agents with a shared secret (Bearer token from `dashboard.toml`).

### paint bot modules

Core loop: `cli.rs` (clap CLI parsing, command dispatch), `live.rs` (live trading loop combining feeds + discovery + strategies + settlement), `config.rs` (all env-configurable settings, `set_param` for sweeps).

Strategies: `strategies/latency_arb.rs` (momentum vs stale odds, adaptive threshold, cooldown), `strategies/spread_capture.rs` (UP ask + DOWN ask < threshold, buys both sides).

Feeds: `feeds/binance_feed.rs` (Binance aggTrade stream), `feeds/clob_feed.rs` (CLOB order book + dynamic resubscription), `feeds/chainlink_feed.rs` (RTDS Chainlink prices + staleness detection), `feeds/util.rs` (exponential backoff with jitter, stable connection tracking).

Data: `bankroll.rs` (per-strategy half-Kelly sizing, caps, confidence curve, DD pause), `position_manager.rs` (trade lifecycle, opposing position guard, settlement), `circuit_breaker.rs` (pause after consecutive losses), `tick_logger.rs` (1s interval tick sampling to SQLite), `trend_tracker.rs` (experimental directional trend filter, off by default).

Backtesting: `backtest/runner.rs` (core replay loop), `backtest/sweep.rs` (parallel parameter sweep via rayon, PID-based temp DBs), `backtest/tick_replay.rs` (loads ticks, groups by 10ms tolerance), `backtest/window_manager.rs` (replays market windows from DB), `backtest/feed_state.rs` (simulated feed state), `backtest/momentum.rs` (rolling window momentum calculator).

Database: `db/database.rs` (rusqlite wrapper, prepared statements, WAL mode), `db/schema.rs` (6 SQLite tables with indexes), `db/build_data.rs` (merges run DBs into market-data.db).

Shared: `types.rs` (Signal, BookState, MarketWindow), `clock.rs` (Clock trait + SystemClock + BacktestClock), `errors.rs` (thiserror error types).

### Agent modules

`api.rs` (10 REST endpoints + WS route), `db_reader.rs` (read-only SQLite connection, status/trades/balance/signals/stats queries), `ws.rs` (DB poller + WebSocket broadcast handler), `process_manager.rs` (ChildProcessManager for bot lifecycle control, NoopProcessManager for monitoring-only mode), `auth.rs` (shared-secret Bearer middleware), `types.rs` (BotStatus, TradeRow, WsMessage), `error.rs` (AgentError with HTTP status mapping).

### Dashboard server modules

`auth.rs` (Argon2 hashing, JWT creation/validation, auth middleware), `config.rs` (TOML config: server port, JWT secret, agents list), `db.rs` (SQLite users/sessions store), `proxy.rs` (HTTP proxy helpers for agent communication), `error.rs` (DashboardError with HTTP status mapping).

API routes: `api/auth_routes.rs` (login, me), `api/bots.rs` (list bots, proxy status/trades/balance/signals/stats/logs/process/start/stop/restart), `api/users.rs` (admin-only user management), `api/ws_proxy.rs` (WebSocket proxy: validates JWT, bridges client <-> agent).

## Project Structure

```
buba-paint/
  Cargo.toml                       # workspace root (3 crates)
  CLAUDE.md                        # AI development guidelines
  Readme.md                        # this file
  docker-compose.yml               # full stack (paint + agent + dashboard)
  dashboard.toml.example           # dashboard config template
  .env.example                     # bot environment variables template
  bots/
    paint/                         # paint bot, BTC Up/Down 5m
      Cargo.toml
      Dockerfile
      src/
        main.rs, lib.rs, cli.rs, live.rs, config.rs, types.rs, ...
        feeds/                     # Binance, CLOB, Chainlink WebSocket feeds
        strategies/                # latency-arb, spread-capture
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
  legacy-ts/                       # archived TypeScript implementation
  scripts/                         # Python analysis scripts + demo DB seed
  data/                            # derived data (reproducible)
  runs/                            # primary live data (IRREPLACEABLE)
    001/ ... 008/                  #   DB, logs, analysis PNGs (LFS)
```

`runs/` contains primary data collected during live paper trading sessions over weeks. This data is irreplaceable. Never delete or modify files in `runs/`. `data/` is derived and reproducible.

## CLI Reference

### Live paper trading (paint)

```bash
cargo run -p buba-paint --release -- live --db-path runs/008/paint.db --balance 200
cargo run -p buba-paint --release -- live --set LATENCY_ARB_MAX_ASK=0.55
```

### Single backtest (paint)

```bash
cargo run -p buba-paint --release -- backtest \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 --set PEAK_DD_PAUSE_PCT=1.0
```

### Parameter sweep (paint)

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-005/sweep.csv
```

`--sweep PARAM=start:end:step` generates a range; `--sweep PARAM=a,b,c` enumerates. `--set PARAM=value` fixes a parameter without sweeping.

### Build merged market data (paint)

```bash
cargo run -p buba-paint --release -- build-data
cargo run -p buba-paint --release -- build-data --runs-dir runs --output data/market-data.db
```

Merges tick data, markets, and trade results from all run DBs into a single database for backtesting. Source DBs opened read-only. Idempotent.

## Configuration

All settings via environment variables or `--set` CLI flag.

Core: `DB_PATH` (default `./data/paint.db`) is the SQLite database path. `LOG_LEVEL` (default `info`): debug, info, warn, error. `TICK_INTERVAL` (default `1000`): tick sampling interval in ms. `GAMMA_POLL_INTERVAL` (default `60000`): Gamma API poll interval in ms. `CHAINLINK_STALE_MS` (default `30000`): force-reconnect after silence.

Latency arb: `LATENCY_ARB_MOMENTUM_THRESHOLD` (default `0.0015`) is the min momentum fraction (0.15%). `LATENCY_ARB_MAX_ASK` (default `0.55`): max ask to consider stale. `LATENCY_ARB_MIN_ASK` (default `0.30`): min ask, rejects cheap tokens. `LATENCY_ARB_COOLDOWN_MS` (default `60000`): cooldown between signals. `MOMENTUM_WINDOW_MS` (default `30000`): momentum rolling window.

Spread capture: `SPREAD_CAPTURE_THRESHOLD` (default `0.998`) is the max UP+DOWN ask sum. `SPREAD_CAPTURE_MIN_ASK` (default `0.15`): reject degenerate books.

Bankroll: `STARTING_BALANCE` (default `150`) is the initial paper balance in USD. `MAX_POSITION_FRACTION` (default `0.10`): max fraction per trade (10%). `MAX_POSITION_USD_FRACTION` (default `0.20`): hard cap per trade (20%). `MIN_BALANCE_THRESHOLD` (default `20`): stop trading below this. `MAX_DRAWDOWN_PCT` (default `0.50`): stop at 50% drawdown.

Kelly criterion: `KELLY_FRACTION` (default `0.5`) is the half-Kelly multiplier. `MIN_WIN_RATE_FOR_KELLY` (default `0.52`): min win rate to apply Kelly. `MIN_TRADES_FOR_KELLY` (default `20`): fixed fraction until enough trades. `KELLY_ROLLING_WINDOW` (default `30`): rolling window per strategy. `MIN_KELLY_FLOOR` (default `0.03`): min fraction floor (3%). `MIN_BET_USD` (default `5`): min bet size in USD.

Position limits and safety: `MAX_OPEN_POSITIONS` (default `5`): max concurrent positions. `MIN_WINDOW_TIME_MS` (default `90000`): don't enter with <90s left. `CIRCUIT_BREAKER_LOSSES` (default `3`): pause after N consecutive losses. `CIRCUIT_BREAKER_PAUSE_MS` (default `900000`): pause duration (15 min). `PEAK_DD_PAUSE_PCT` (default `0.30`): pause at 30% drawdown from peak. `PEAK_DD_PAUSE_MS` (default `3600000`): DD pause duration (1 hour). `DD_PAUSE_RECOVERY_PCT` (default `0.05`): DD must recover by 5% before re-arming. `RECONNECT_MIN_STABLE_MS` (default `5000`): min connection duration to reset backoff. `RECONNECT_MAX_FAILURES` (default `20`): feed circuit breaker threshold. `RECONNECT_PAUSE_MS` (default `300000`): feed circuit breaker pause (5 min).

Trend filter (experimental, off by default): `TREND_FILTER_ENABLED` (default `false`): enable counter-trend suppression. `TREND_FILTER_THRESHOLD` (default `0.30`): bias threshold to suppress. `TREND_FILTER_WINDOW` (default `10`): recent outcomes to consider.

## Database Schema

SQLite (WAL mode). Python scripts can read concurrently while the bot writes.

- tick_data: 1-second sampled prices from all feeds. Columns: timestamp (Unix ms), source (binance/clob_up/clob_down/chainlink), price, bid, ask, bid_size, ask_size.
- markets: one row per 5-minute window. Columns: market_id (Gamma API ID), question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status (active/closed/resolved).
- signals: every strategy detection event. Linked to the market and strategy that generated it, includes prices and book state at detection time.
- simulated_trades: opened positions. Links to market and signal, tracks entry price, size, token, side, and status (open/settled).
- trade_results: settlement P&L. Links to trade, records exit price, settlement price, and PnL at 0-3% fee tiers.
- balance_log: bankroll history. Records every balance change with timestamp, event type, trade reference, amount, and running balance.

See `bots/paint/src/db/schema.rs` for full DDL.

## Backtesting

The tick-level backtester replays historical data through the real strategy code. Ticks are loaded into memory, grouped by 10ms tolerance, then replayed through strategies, bankroll, and settlement, identical to the live path.

### Sweep Results

- 001 (TS): ~40 min, best PnL $14,602. INVALID, stale temp DBs inflated balance.
- 002 (TS): ~40 min, best PnL $1,282. Peak DD pause throttled at $200 start.
- 003 (TS): ~40 min, best PnL $4,070. Baseline, DD pause disabled, matches live.
- rust-004: 42s, best PnL $4,070. Rust parity validation (identical to 003).
- rust-005: 42s, best PnL $4,070. Post-refactor, identical to rust-004.
- rust-006: 4 min, best PnL $815,678. Full range (Feb 15 to Mar 20, 8.8M ticks).

Always use `--set PEAK_DD_PAUSE_PCT=1.0` and `--set SPREAD_CAPTURE_THRESHOLD=0.50` in sweeps. DD pause is too aggressive for small starting balances, and spread-capture overcounts ~18x due to 1s tick sampling.

## Run History

- Run 001: v0.0, first test run.
- Run 002: 5h, 9 trades, 55.6% WR, +$69. v0.1, fixed 100-token bets.
- Run 003: 1h, 1 trade, 0% WR, -$11. v0.2, bankroll-aware sizing.
- Run 004: 96h, 76 trades, 51.3% WR, +$719, $200->$919, peak $1,556. v0.2.
- Run 005: 25h, 11 trades, 36.4% WR, -$5. v0.3, over-filtering bug.
- Run 006: 267h, 292 trades, 56.5% WR, +$4,565, $200->$4,765, peak $9,678. v0.4.
- Run 007: 222h, 187 trades, 56.7% WR, +$5,488, $200->$5,688, peak $8,130. v0.5 TS.
- Run 008: ongoing, v0.6 Rust, $200 start, deployed Mar 20.

## Testing

874 tests total: 743 Rust + 131 frontend.

- paint bot: 569 tests (517 unit + 52 integration). Unit tests in `src/*/tests/` via `#[path]` attribute. Integration tests in `tests/` use mock WebSocket servers and wiremock for HTTP.
- agent: 92 tests (89 unit + 3 integration). Covers REST endpoints, WS polling/broadcast, DB reader, process manager, auth middleware.
- dashboard server: 82 tests (80 unit + 2 integration). Covers auth/JWT, bot proxy handlers, WS proxy, config parsing, user management, error mapping.
- dashboard client: 131 tests across 27 files. Covers API client, WS connection, auth store, all hooks, components, and pages. Uses vitest + @testing-library/react + jsdom.

```bash
cargo test                              # all 743 Rust tests
cargo clippy --workspace -- -D warnings # lint
cargo fmt --all --check                 # format check
cd dashboard/client && npx vitest run   # all 131 frontend tests
```

TDD is strictly enforced. When a test fails, fix the code, not the test.

## Deployment

```bash
# Docker Compose (recommended)
cp dashboard.toml.example dashboard.toml # edit agents + secrets
docker compose up -d

# Manual deploy
cargo build --release
scp target/release/buba-paint buba-paint:~/bot/
scp target/release/buba-agent buba-paint:~/bot/
./buba-paint live --db-path runs/008/paint.db --balance 200
./buba-agent --db-path runs/008/paint.db --port 9090
```

The paint bot runs indefinitely, rolling through 5-minute windows. Ctrl+C for graceful shutdown (final stats printed, feeds disconnect, DB closes).

## Analysis Scripts

```bash
python3 scripts/pnl_curve.py              runs/006/buba-paint.db
python3 scripts/latency_distribution.py   runs/006/buba-paint.db
python3 scripts/spread_over_time.py       runs/006/buba-paint.db
python3 scripts/signal_frequency.py       runs/006/buba-paint.db
python3 scripts/binance_vs_chainlink.py   runs/006/buba-paint.db
```

Requires Python 3 with matplotlib, pandas, numpy. Each script produces a `.png` and an interactive matplotlib window.

## Cross-compilation

- Dev: macOS aarch64 (Apple Silicon)
- Prod: Linux aarch64 (AWS t4g.micro, Ubuntu 24.04)
