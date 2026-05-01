# Documentation Index

This directory keeps durable project guidance only. Active implementation plans belong in the repository root. Run-specific analysis belongs under `data/experiments/...`.

## Current System Docs

- [system-architecture.md](./system-architecture.md): services, data flow, strategy runtime, feeds, dashboard IA, and current execution modes.
- [commands-and-config.md](./commands-and-config.md): common commands, environment knobs, backtest/sweep usage, and local stack commands.
- [data-and-replay.md](./data-and-replay.md): data ownership, replay-grade capture, DB tables, backtesting, and useful SQL.
- [testing-and-validation.md](./testing-and-validation.md): test lanes, comment policy, docs audit, coverage, and live-money validation discipline.
- [deployment-and-ops.md](./deployment-and-ops.md): local runtime, remote layout, staging, process model, remote checks, and cleanup policy.
- [live-trading-architecture.md](./live-trading-architecture.md): current live-readonly and future live-trading architecture.
- [live-session-runbook.md](./live-session-runbook.md): intended operator workflow for future real-money sessions.
- [polymarket-live-constraints.md](./polymarket-live-constraints.md): venue constraints that must be revalidated before funded deployment.
- [pending-settlement-modes.md](./pending-settlement-modes.md): reserve-mode semantics and exact-run replay guidance.
- [runs.md](./runs.md): local run index and quality notes.

## Archive

- [archive/Readme.md](./archive/Readme.md): archive index.
- [archive/live-readiness-review.md](./archive/live-readiness-review.md): completed readonly readiness-review snapshot kept for provenance.
