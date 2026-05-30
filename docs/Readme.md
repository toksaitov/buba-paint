# Buba Documentation

This directory is the stable reference for the system. Read it from the top when you need to rebuild context without chat history. It describes the current system, not the order in which the system was built.

Root planning files are temporary while work is unfinished. Run-specific investigations under `data/experiments/...` and sweep outputs under `data/sweeps/...` are provenance, not current guidance.

## Reading Order

Read these chapters in order when reconstructing the project from repository state:

1. [../Readme.md](../Readme.md): purpose, current operating posture, services, data labels, and first commands.
2. [system-architecture.md](./system-architecture.md): service boundaries, data/control flow, hot-path rules, dashboard shape, and failure boundaries.
3. [strategy-and-risk.md](./strategy-and-risk.md): strategy families, current latency-only readonly profile, bankroll/risk controls, and future live-money constraints.
4. [data-and-replay.md](./data-and-replay.md): replay-grade capture, validation classes, SQLite block storage, backtest readiness, and sweep preparation.
5. [deployment-and-ops.md](./deployment-and-ops.md): Docker/Caddy process model, remote layout, local stacks, safe partial redeploys, cleanup, and acceptance checks.
6. [testing-and-validation.md](./testing-and-validation.md): test lanes, low-latency evidence, docs fact-checking, and readiness gates.
7. [polymarket-live-constraints.md](./polymarket-live-constraints.md), [live-trading-architecture.md](./live-trading-architecture.md), and [live-session-runbook.md](./live-session-runbook.md): advanced future-funded context.

## Audience Paths

Traders and operators should read:

* [../Readme.md](../Readme.md)
* [strategy-and-risk.md](./strategy-and-risk.md)
* [deployment-and-ops.md](./deployment-and-ops.md)
* [live-session-runbook.md](./live-session-runbook.md)
* [polymarket-live-constraints.md](./polymarket-live-constraints.md)

Analysts and researchers should read:

* [data-and-replay.md](./data-and-replay.md)
* [strategy-and-risk.md](./strategy-and-risk.md)
* [pending-settlement-modes.md](./pending-settlement-modes.md)
* [runs.md](./runs.md)
* [../data/sweeps/Readme.md](../data/sweeps/Readme.md)

Developers should read:

* [system-architecture.md](./system-architecture.md)
* [commands-and-config.md](./commands-and-config.md)
* [testing-and-validation.md](./testing-and-validation.md)
* [../CLAUDE.md](../CLAUDE.md)

Operators responsible for deployment should read:

* [deployment-and-ops.md](./deployment-and-ops.md)
* [../ops/docker/Readme.md](../ops/docker/Readme.md)
* [testing-and-validation.md](./testing-and-validation.md)

LLM agents should read:

* [../Readme.md](../Readme.md)
* this index
* [../CLAUDE.md](../CLAUDE.md)
* the two or three domain chapters relevant to the task

Do not use chat history as system truth. Rebuild context from this repository and verify claims against code, config, validators, or official venue docs.

## Truth Sources

Documentation explains durable design. Runtime truth comes from executable state:

* commands and gates: `Makefile`
* deployment wiring: `docker-compose.yml`, `docker-compose.paper.yml`, `docker-compose.live-readonly.yml`, `docker-compose.prod.yml`, and `scripts/deploy-docker.py`
* bot config defaults and env overrides: `bots/paint/src/config.rs`
* CLI behavior: `bots/paint/src/cli.rs`
* dashboard routes and APIs: `dashboard/client/src/lib/routes.ts`, `dashboard/client/src/lib/api.ts`, `dashboard/server/src/main.rs`, and `agent/src/main.rs`
* sidecar routes and SDK versions: `polymarket-sidecar/src/server.ts`, `polymarket-sidecar/src/config.ts`, and `polymarket-sidecar/package.json`
* deployed runtime config: dashboard Parameters page, `GET /api/runtime/config`, or `run_metadata.runtime_config_snapshot`
* machine state: dashboard Machine page or `GET /api/machine`
* research host telemetry: `GET /api/research/machines/:id/telemetry` and the `research_machine_telemetry_*` tables
* data quality: `validate-replay-data`, `validate-backtest-input`, `prepare-backtest-input`, and `validate-live-fidelity`

When docs and code disagree, code wins until the docs are corrected. When local venue assumptions and official Polymarket docs disagree, official docs win until production-safe readonly checks prove otherwise.

## Current Stable Chapters

* [system-architecture.md](./system-architecture.md): system narrative and service boundaries.
* [strategy-and-risk.md](./strategy-and-risk.md): strategy logic, current profile, risk controls, and future canary limits.
* [commands-and-config.md](./commands-and-config.md): common CLI, env groups, Docker targets, backtest/sweep commands, and live control commands.
* [data-and-replay.md](./data-and-replay.md): storage profiles, validation classes, SQLite layout, CLOB replay blocks, and sweep readiness.
* [deployment-and-ops.md](./deployment-and-ops.md): preferred Docker/Caddy deployment and operational procedures.
* [testing-and-validation.md](./testing-and-validation.md): required checks, coverage lanes, mobile/PWA testing, docs fact checks, and low-latency gates.
* [polymarket-live-constraints.md](./polymarket-live-constraints.md): official venue constraints and local sidecar implications.
* [live-trading-architecture.md](./live-trading-architecture.md): advanced live-runtime, ledger, sidecar, control, and halt architecture.
* [live-session-runbook.md](./live-session-runbook.md): future funded-session workflow and closeout expectations.
* [pending-settlement-modes.md](./pending-settlement-modes.md): settlement timing and reserve semantics.
* [runs.md](./runs.md): historical run index and quality notes.

## Provenance And Archives

* [archive/Readme.md](./archive/Readme.md): archived documentation index.
* [archive/live-readiness-review.md](./archive/live-readiness-review.md): old readiness snapshot retained as provenance.
* `data/experiments/...`: incident analysis, deployment evidence, and one-off investigations.
* `data/sweeps/...`: sweep outputs and parameter-search notes.

Archive and experiment docs may describe old decisions. They should not be treated as operating instructions unless a current stable doc points to a specific artifact for evidence.
