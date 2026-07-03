# Buba

Buba is a trading system for short-horizon prediction markets. Its goal is to run a strategy from observation to execution with the same discipline expected of a small exchange-facing system: bounded decision latency, explicit risk limits, reproducible data capture, and enough evidence after a run to replay what the bot saw.

The current implementation focuses on Polymarket BTC 5-minute Up/Down markets. It can run paper sessions, run authenticated readonly sessions against Polymarket, collect replay-grade market data, show an operator dashboard, and run offline backtests and parameter sweeps. No real money has traded: the live-trading path is real-venue capable but ships disarmed and dry-run by default.

Active effort: live-money readiness. All phase code is on `master` (pushed to `origin`), complete and hardened, and was reviewed SAFE FOR CANARY; the only remaining gate is an explicit operator GO, which then runs a final supervised dry-run rehearsal and one small real canary order. The live host is currently staged in the disarmed, dry-run canary stack, parked. Start from the "Current State And Handoff" block at the top of [LIVE_READINESS_PLAN.md](./LIVE_READINESS_PLAN.md).

## System Overview

Buba is organized around one latency-sensitive bot and four supporting services.

* The Rust bot ingests feeds, maintains in-memory market state, evaluates strategies, simulates paper execution, and writes the run database.
* The TypeScript sidecar holds Polymarket credentials and performs authenticated CLOB V2 account, preflight, activity, order, cancel, and redemption calls.
* The agent reads the run database, tails bounded logs, samples machine state, and exposes REST and WebSocket APIs.
* The dashboard server authenticates users and proxies agent APIs.
* The dashboard client presents execution state, logs, parameters, machine health, trades, signals, strategies, and equity history.

The bot owns trading decisions. The sidecar owns venue credentials. The agent and dashboard observe the system and queue controls; they must not become dependencies of feed handling or strategy evaluation.

## Strategies

`paint`, the active bot, evaluates three strategy families.

* `latency-arb` trades when Binance BTC moves before Polymarket reprices.
* `spread-capture` buys both UP and DOWN when the combined asks are cheap enough after fees. Its legs are independent, so one-leg residual exposure is a real risk.
* `calm-persistence` trades quiet late-window persistence when the currently winning side appears underpriced.

The current remote profile is latency-only: `LATENCY_ARB_ENABLED=true`, `SPREAD_CAPTURE_ENABLED=false`, `CALM_PERSISTENCE_ENABLED=false`, replay-grade capture, and a `$100` shadow balance. Confirm live values from the dashboard Parameters page or `run_metadata.runtime_config_snapshot`.

## Execution Modes

`paper` runs the shared strategy engine with simulated execution.

`live_readonly` adds authenticated sidecar account, preflight, market metadata, user activity, and reconciliation reads while continuing shadow paper trading. It does not submit venue orders.

`live_trading` is a real-venue runtime that ships disarmed. It has audited controls, live ledger tables, terminal halt state, and durable decision evidence. It is currently staged on the live host in the disarmed, dry-run canary configuration; remote arming still requires an explicit operator GO. See [LIVE_READINESS_PLAN.md](./LIVE_READINESS_PLAN.md) and [CANARY_RUNBOOK.md](./CANARY_RUNBOOK.md).

A running process is not an armed process. Sidecar write endpoints and dashboard controls are not permission to trade real money.

## Data Model

The run database is SQLite. It stores market windows, public feed replay inputs, strategy signals, rejection summaries, simulated trades, balance history, live-readonly account snapshots, and live ledger rows when applicable.

Use the data labels exactly:

* `replay_grade` is the configured storage profile for public decision inputs.
* `sweep_grade` means an interval actually contains the required public feed classes.
* `backtest_ready` means the current backtester can load and dry-run the interval.
* `prepared_backtest` means a derived database has been copied, validated, and indexed for large sweeps.
* `research_grade_live` means a funded live interval has complete private lifecycle, account, control, and reconciliation evidence.

The trading loop does not run replay validators, SQLite `quick_check`, whole-table scans, or dashboard summaries. Those checks are offline gates.

## Quick Deploy

The normal remote deployment is Docker Compose with Caddy TLS in `live_readonly` mode. It runs the bot, sidecar, agent, dashboard, and Caddy on `buba-paint`; only Caddy publishes public ports.

Prerequisites:

* `ssh buba-paint` works from the operator machine.
* `buba.toksaitov.com` points at the `buba-paint` host.
* inbound TCP `80` and `443` are open.
* `.secrets/buba-paint-live-sidecar.env` exists locally and is not committed.

Preview and deploy:

```bash
make docker-deploy-dry-run
make docker-deploy
```

The default deployment target is `live_readonly` on `https://buba.toksaitov.com`. It is for authenticated readonly monitoring and shadow paper trading, not arming real money. Details, cleanup, and partial-redeploy commands are in [docs/deployment-and-ops.md](./docs/deployment-and-ops.md) and [ops/docker/Readme.md](./ops/docker/Readme.md).

## Operation

Remote operation uses Docker Compose with Caddy.

* Caddy is the only public edge and publishes ports `80` and `443`.
* Caddy provisions TLS and reverse-proxies the dashboard.
* bot, sidecar, agent, and dashboard stay on a private Compose network.
* runtime DBs and logs live under `~/buba-paint-live/runtime/<runtime-name>`.
* stable config and Caddy state live under `~/buba-paint-live/config` and `~/buba-paint-live/caddy`.

## Local Use

Build and run core checks:

```bash
cargo build --release
cargo test
make lint
make docs-audit

cd dashboard/client && npm install && npm test
cd polymarket-sidecar && npm install && npm test
```

Start the local paper stack:

```bash
mkdir -p .docker/runtime
docker compose -f docker-compose.yml -f docker-compose.paper.yml -f docker-compose.local.yml up -d --build
```

Validate replay and backtest inputs:

```bash
cargo run -p buba-paint --release -- validate-replay-data --data /path/to/paint.db --start <time> --end <time>
cargo run -p buba-paint --release -- validate-backtest-input --data /path/to/paint.db --start <time> --end <time>
cargo run -p buba-paint --release -- prepare-backtest-input --data /path/to/paint.db --start <time> --end <time> --output /tmp/prepared.db
```

Use `/tmp` for scratch databases and evidence. Do not put temporary DB, WAL, SHM, log, or readiness artifacts in the repository root.

## Documentation

Start with [docs/Readme.md](./docs/Readme.md). The main chapters are:

* [docs/system-architecture.md](./docs/system-architecture.md): services, data flow, hot-path boundaries, and dashboard shape.
* [docs/strategy-and-risk.md](./docs/strategy-and-risk.md): strategies, current enablement, risk controls, and future canary posture.
* [docs/data-and-replay.md](./docs/data-and-replay.md): replay storage, validation classes, backtest readiness, and sweep preparation.
* [docs/deployment-and-ops.md](./docs/deployment-and-ops.md): Docker/Caddy deployment, remote layout, partial redeploys, and runtime checks.
* [docs/testing-and-validation.md](./docs/testing-and-validation.md): test lanes, low-latency gates, docs fact-checking, and readiness evidence.
* [docs/polymarket-live-constraints.md](./docs/polymarket-live-constraints.md): venue facts that must be revalidated before funded trading.

Agent instructions are in [CLAUDE.md](./CLAUDE.md). [AGENTS.md](./AGENTS.md) is only a compatibility alias.

Stable documentation belongs under `docs/`. Root planning files are temporary and should be removed when closed. Run evidence belongs under `data/experiments/...`, sweep outputs under `data/sweeps/...`, and primary run data under `runs/`.
