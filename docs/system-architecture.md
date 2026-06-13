# System Architecture

This chapter explains how the system is put together. It is written for readers who need to understand the current repository without reading implementation-history notes.

## Overview

Buba is a single-bot trading system where the Rust bot owns decisions and the run DB, a TypeScript sidecar owns private venue calls, an agent exposes bounded read APIs, and a dashboard gives humans an authenticated operator surface.

## Service Boundaries

`bots/paint` is the trading and research core. It discovers BTC 5-minute markets, subscribes to public feeds, maintains the in-memory decision state, evaluates strategy candidates, simulates paper execution, writes SQLite run data, and provides backtest, sweep, validation, and closeout CLIs.

`polymarket-sidecar` is the private venue boundary. It isolates Polymarket credentials and authenticated calls from the bot. Its public local routes are `GET /health`, `GET /account`, `GET /activity`, `POST /preflight`, `POST /orders`, `POST /cancel`, `POST /cancel-all`, and `POST /redeem-all`.

`agent` is a monitor and control adapter. It reads the bot DB, tails bounded logs, samples machine state, exposes REST and WebSocket APIs, and forwards bot control requests through the bot DB. It does not decide trades.

`dashboard/server` authenticates dashboard users, serves the built frontend, and proxies configured agent APIs.

`dashboard/client` is the operator UI. Its Monitor pages are Overview, Execution, Logs, Parameters, and Machine. Its Analysis pages are Trend, Trades, Signals, and Strategies. Its Research pages currently include Overview, Machines, Artifacts, Transfers, Jobs, and Reports. Compatibility redirects map `/live` and `/trading` to Execution, `/config` to Parameters, and `/stats` to Strategies.

## Runtime Flow

Paper and readonly runs follow the same core loop:

1. The bot discovers active BTC 5-minute markets through Gamma/market metadata.
2. Binance, CLOB, and Chainlink feed events update in-memory state.
3. Replay-grade feed evidence is queued to bounded persistence workers.
4. The decision worker evaluates the latest coalesced state.
5. Paper execution simulates order arrival, fill, miss, and settlement behavior.
6. The bot persists signals, decision evidence, rejection summaries, trades, balances, markets, feed health, and runtime metadata.
7. The agent reads the run DB and runtime files.
8. The dashboard renders operator and analysis views.

`live_readonly` adds authenticated sidecar account, preflight, market metadata, user activity, and reconciliation reads. It still runs shadow paper trading and does not place venue orders.

`live_trading` exists locally as a disarmed runtime. If armed in a local verification environment, live order submission goes through a submission worker that persists critical decision evidence and order intent before any sidecar venue call. Unknown order outcomes, capture failures, stale account truth, user-stream failure, or reconciliation-critical state block new risk.

## Execution Modes

`paper` is the normal simulated research mode.

`live_readonly` is the normal production-like remote mode. It authenticates against the sidecar, monitors account and venue state, captures replay-grade public feeds, and runs shadow paper strategies. It does not arm or write venue state.

`live_trading` starts disarmed and requires audited local or admin dashboard live-control commands. It has ledger tables, risk halt state, reconciliation, and sidecar write support, but remote real-money arming is deferred.

Process state and trading state are separate. A running process is not an armed process.

## Hot Path

The hot path owns:

* current public feed state
* current market and window state
* signal-feature state
* strategy state
* bankroll and exposure state
* submission eligibility

The hot path may enqueue bounded work. It must not run SQLite scans, SQLite integrity checks, replay validators, dashboard aggregation, sidecar calls, account refresh, control polling, settlement fetches, or direct venue submission.

Workers handle:

* replay-grade feed persistence
* CLOB replay block compression
* compact decision evidence
* paper analytics persistence
* live order intent durability and sidecar submission
* account, preflight, and activity refresh
* live-control polling
* settlement and resolution checks
* runtime capture-health metadata

If workers lag, queues fill, persistence fails, or remote truth becomes unknown, `live_trading` blocks new submissions. Dashboard freshness may degrade before bot latency degrades.

## Feed Model

The bot consumes these public inputs:

* Binance `aggTrade`
* Binance `bookTicker`
* Binance depth stream
* Polymarket CLOB market WebSocket data for UP and DOWN tokens
* Polymarket RTDS Chainlink BTC/USD context
* Gamma and CLOB market metadata

Feed reconnect knobs reduce downtime. They must not weaken stale-data gates or permit blind trading. CLOB messages may lack useful source timestamps, so the bot preserves raw source timestamps when available and uses local receive time for freshness decisions.

## Dashboard Model

The dashboard is deliberately split between monitoring and analysis.

Monitor pages:

* Overview: triage, simulated performance, current market, open trades, and recent outcomes.
* Execution: process mode, execution readiness, account state, reconciliation, control audit, and gated future controls.
* Logs: bounded log tail with search, severity/source/event filters, follow, wrap, and line-count preferences.
* Parameters: read-only sanitized runtime config snapshot from `run_metadata.runtime_config_snapshot`.
* Machine: CPU, memory, swap, disk, and runtime DB/WAL/SHM file-size monitoring from the agent sampler.

Analysis pages:

* Trend: shadow equity curve.
* Trades: simulated trade and settlement review.
* Signals: signal metrics, grouped bursts, and rejection investigation.
* Strategies: strategy-family contribution and risk context.

Research pages:

* Overview: cross-entity counts, recent jobs, transfers, and reports.
* Machines: read-only observability for research-role hosts, with machine status, stale or missing telemetry state, worker heartbeat and activity, dependency counts, and host telemetry detail. Machine create, edit, disable, enable, and delete remain backend/API/script operations only.
* Artifacts: exported run packages with manifest, checksum, verification, and metadata-only or files-included deletion.
* Transfers: artifact transfers with progress, stale detection, checksum status, pause/resume/cancel/retry/verify lifecycle.
* Jobs: export, backtest, and sweep jobs with step timeline, embedded event stream, blocked/failed recovery, clone, and regenerate report.
* Reports: backtest and sweep reports with summary metrics, equity curve, sweep points, and JSON/CSV inspection.

The Research section is observation-and-steering only. Step leasing and rsync execution happen in the `buba-research-worker` process on each host; the dashboard never runs jobs directly. The worker leases durable steps through a backend that is either the local SQLite database or a remote controller reached over the worker-token API at `/api/research/workers/*`. Adaptive polling pulls at 3 seconds for active jobs and transfers and 10 seconds for terminal entities. The canonical lifecycle state machine lives at `dashboard/client/src/lib/research-permissions.ts` and is the single source of truth for which controls render. Operator-facing API surface for these pages lives under `/api/research/*` in `dashboard/server/src/api/research.rs`. The control-plane design, backend model, worker-token API, lifecycle state machines, and report schema version 2 are described in [research-orchestration.md](./research-orchestration.md).

Research machine identity comes from dashboard state and `ops/research-machines.toml`. Docker Compose services are deployment and health evidence, not the source of machine identity. Research pages use machines as source, destination, worker health, and provenance context for artifacts, transfers, jobs, and reports.

Machine telemetry is separate from identity and management. The shared `buba-machine-telemetry` crate defines host identity, host samples, and sampler health for both the live bot agent and research workers. The agent keeps `GET /api/machine` response-compatible for Monitor > Machine. The research worker writes typed telemetry into `research_machine_telemetry_state` and `research_machine_telemetry_samples`. When configured with a controller URL and worker token, the worker also reports telemetry and leases all job, step, transfer, report, and artifact work through that controller, not only heartbeats. `GET /api/research/machines/:id/telemetry` returns machine metadata, latest telemetry, bounded sample history, dependency counts, disabled state, stale state, and the stale threshold.

The dashboard does not call the sidecar or venue directly. Live-control mutations route through dashboard server, agent, control ledger, and the running bot.

## Failure Boundaries

The system intentionally fails closed:

* A failed sidecar account/preflight/activity state degrades readiness and blocks live risk.
* Matching-engine restart responses become venue-degraded state, not successful submissions.
* Unknown submission outcomes remain `unknown_order` until reconciled.
* Terminal live halt state cannot be cleared by a normal disarm.
* A halted DB must not be re-armed.
* Runtime validators and whole-run scans stay offline and must not run inside the trading loop.

## Code Map

Paint bot:

* `live.rs`: shared runtime, readonly sessions, live-control application, and disarmed live-trading state.
* `live_decision.rs`: pure decision evidence and in-memory live decision handling.
* `live_feed_writer.rs`: bounded feed-event persistence worker.
* `live_storage.rs`: replay-grade compaction rules.
* `config.rs`: execution mode, storage profile, strategy knobs, live caps, queue budgets, and sidecar timeouts.
* `strategies/`: latency-arb, spread-capture, and calm-persistence logic.
* `signal_features.rs`: feature engine shared by runtime and replay.
* `backtest/`: replay, feed state, window manager, runner, and sweep logic.
* `db/`: schema, migrations, block storage, validators, and derived-data tools.

Other services:

* `crates/buba-machine-telemetry`: shared host identity, sample, sampler health, and ring-buffer telemetry contract.
* `agent/src/main.rs`: agent routes.
* `dashboard/server/src/main.rs`: dashboard server routes.
* `dashboard/server/src/research_backend.rs`: the `ResearchWorkBackend` trait and the local `DashboardDb` implementation the worker leases through.
* `dashboard/server/src/research_controller_client.rs`: the remote `ResearchControllerClient` and `WorkerBackend` enum that route worker operations over the worker-token HTTP API.
* `dashboard/server/src/api/research_workers.rs`: token-authenticated `/api/research/workers/*` endpoints for step, job, artifact, report, transfer, and machine operations.
* `dashboard/server/src/bin/buba-research-worker.rs`: research worker loop and artifact transfer execution, selecting a local-database or remote-controller backend at startup, with local telemetry persistence and controller reporting.
* `dashboard/client/src/lib/routes.ts`: dashboard page metadata.
* `dashboard/client/src/lib/api.ts`: frontend API client.
* `polymarket-sidecar/src/server.ts`: sidecar HTTP routes.
* `polymarket-sidecar/src/config.ts`: sidecar environment model.

Use [commands-and-config.md](./commands-and-config.md), [data-and-replay.md](./data-and-replay.md), and [deployment-and-ops.md](./deployment-and-ops.md) for operational details.
