# buba

`buba` is a Rust-first trading research workspace for Polymarket prediction markets. The first bot, `paint`, trades 5-minute BTC Up/Down markets in paper mode and supports authenticated readonly venue monitoring. The workspace includes the bot, a monitoring agent, a dashboard server, a React dashboard client, and a TypeScript Polymarket sidecar.

Current state:

- `paper` is the production research mode.
- `live_readonly` is real authenticated venue/account monitoring plus the shared shadow paper runtime.
- `live_trading` is still gated. It cannot place real orders yet.
- Replay-grade public feed capture is the default for new research runs.
- Active live-money implementation planning lives in [LIVE_TRADING_PLAN.md](./LIVE_TRADING_PLAN.md), not in `docs/`.

Start with [docs/Readme.md](./docs/Readme.md) for durable system documentation. Repository agent instructions live in [CLAUDE.md](./CLAUDE.md); [AGENTS.md](./AGENTS.md) is only a compatibility alias.

## Quick Start

```bash
cargo build --release
cargo test
make lint
make docs-audit

cd dashboard/client && npm install && npm test
cd polymarket-sidecar && npm install && npm test
docker compose up -d
```

Requires Rust 1.94+ and Node 22+ for local frontend and sidecar work. Docker Compose starts a local paper stack with paint, agent, and dashboard. It does not start the Polymarket sidecar or authenticated `live_readonly` monitoring.

## Where To Read Next

- [docs/system-architecture.md](./docs/system-architecture.md): services, data flow, strategy families, dashboard IA, and current execution modes.
- [docs/commands-and-config.md](./docs/commands-and-config.md): common CLI commands, environment knobs, and local stack commands.
- [docs/data-and-replay.md](./docs/data-and-replay.md): run data, replay-grade capture, DB schema, backtesting, and sweep safety.
- [docs/testing-and-validation.md](./docs/testing-and-validation.md): lint, tests, coverage, comment policy, and acceptance gates.
- [docs/deployment-and-ops.md](./docs/deployment-and-ops.md): local and remote process model, staging, server checks, and cleanup policy.
- [docs/live-trading-architecture.md](./docs/live-trading-architecture.md): current live-readonly and future live-trading architecture.
- [docs/polymarket-live-constraints.md](./docs/polymarket-live-constraints.md): venue facts that must be revalidated before funded deployment.
- [docs/live-session-runbook.md](./docs/live-session-runbook.md): intended operator workflow for a future real-money session.
- [docs/pending-settlement-modes.md](./docs/pending-settlement-modes.md): reserve accounting and exact-run replay semantics.
- [docs/runs.md](./docs/runs.md): local run index and historical quality notes.

## Safety State

The sidecar implements real readonly-safe endpoints: `/health`, `/account`, and `/preflight`. Write endpoints remain intentionally non-live. The dashboard Execution page is an operator cockpit for process, mode, account, readiness, and future controls, but those controls remain gated until the live venue runtime exists.

New runs intended for research should keep:

```bash
FEED_EVENT_STORAGE_PROFILE=replay_grade
```

Run `buba-paint validate-replay-data` before any long sweep. Sweeps refuse non-sweep-grade inputs. Old runs that lack required Binance book state are descriptive evidence only, not trusted optimization inputs.

## Main Commands

```bash
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 200
cargo run -p buba-paint --release -- live-preflight
cargo run -p buba-paint --release -- backtest --data data/market-data.db --start 2026-02-20 --end 2026-03-04
cargo run -p buba-paint --release -- sweep --data data/market-data.db --start 2026-02-20 --end 2026-03-04 --output data/sweeps/test/sweep.csv
cargo run -p buba-paint --release -- validate-replay-data --data data/market-data.db --start 2026-02-20 --end 2026-03-04
cargo run -p buba-paint --release -- db-footprint --db-path /tmp/paint.db
```

Never create temporary or test databases in the project root. Use `/tmp` or test tempfiles.

## Data Preservation

`runs/` contains primary run data and should not be edited manually. `data/` contains derived experiments, sweeps, caches, and merged data that are reproducible but still useful. Database files should stay out of Git and LFS history.

## Active Work

Root active-plan files are allowed when work is unfinished and intentionally visible. Stable docs must not contain active implementation plans. The current active plan is [LIVE_TRADING_PLAN.md](./LIVE_TRADING_PLAN.md). Delete it when the live-trading work is complete and move only durable facts into `docs/`.
