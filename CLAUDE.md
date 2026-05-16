# CLAUDE.md

Canonical repository instructions for coding agents. [AGENTS.md](./AGENTS.md) is only a compatibility alias.

## Context Reconstruction

Do not rely on chat history. Rebuild context from repository files:

1. [Readme.md](./Readme.md)
2. [docs/Readme.md](./docs/Readme.md)
3. [docs/system-architecture.md](./docs/system-architecture.md)
4. [docs/strategy-and-risk.md](./docs/strategy-and-risk.md)
5. [docs/data-and-replay.md](./docs/data-and-replay.md)
6. [docs/deployment-and-ops.md](./docs/deployment-and-ops.md)
7. [docs/testing-and-validation.md](./docs/testing-and-validation.md)

Use executable truth for factual claims: `Makefile`, Compose files, `bots/paint/src/config.rs`, `bots/paint/src/cli.rs`, dashboard route/API files, agent routes, sidecar config/server files, DB schema, validators, and current official Polymarket docs for venue behavior.

## Repository Map

* `bots/paint`: Rust bot runtime, strategies, feeds, SQLite persistence, backtests, sweeps, live-readonly, disarmed live-trading, live-control, and replay validators.
* `polymarket-sidecar`: TypeScript authenticated Polymarket boundary for CLOB V2 auth, account/preflight/activity, FOK/FAK orders, cancellation, and redemption.
* `agent`: read-only monitor over the bot DB, runtime logs, process control, and machine state.
* `dashboard/server`: authenticated dashboard backend and agent proxy.
* `dashboard/client`: React operator dashboard.
* `docs/`: stable system documentation.
* `ops/`: deployment artifacts, with Docker/Caddy preferred and systemd retained as legacy reference.
* `scripts/`: repo automation, deploy runners, audits, profiling, and smoke gates.
* `runs/`: primary run data. Do not edit manually.
* `data/`: derived experiments, sweeps, caches, and reports.

Current operating posture: `paper` and Docker/Caddy `live_readonly` are normal. Real-money trading is deferred and requires a fresh explicit plan before arming.

## Build And Test

```bash
cargo build
cargo build --release
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test
make lint
make comment-audit
make docs-audit
make hot-path-audit
make test-fast
make test-integration
make test-slow
make test-e2e
make test-all
make coverage
make coverage-gate

cd dashboard/client && npm test && npm run build
cd polymarket-sidecar && npm test && npm run build
```

Use [docs/commands-and-config.md](./docs/commands-and-config.md) for command details and environment knobs.

## Engineering Rules

1. No `unwrap()` or `expect()` in library code. Use `anyhow::Result` or typed errors. Tests and thin `main()` bootstraps are the only normal exceptions.
2. Use `f64`, never `f32`, for prices, balances, probabilities, and fractions.
3. Config is immutable after construction. Pass `&Config`.
4. Time is injectable through the `Clock` trait where behavior needs deterministic tests.
5. SQL belongs in `bots/paint/src/db/` or the owning DB boundary. Do not scatter raw SQL through runtime logic.
6. Strategies are stateful structs implementing the strategy interfaces already present in the bot.
7. Unit tests live under `src/*/tests/` via `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`. Integration tests live under crate-level `tests/`.
8. TDD is expected. When a test fails, fix the code unless the expected value is demonstrably wrong.
9. Frontend tests use Vitest and React Testing Library, colocated in `__tests__/` directories.
10. After behavior, schema, API, deployment, or workflow changes, update the relevant docs.

## Hot-Path Rules

The trading hot path may update in-memory state, compute features, evaluate strategy candidates, and enqueue bounded work. It must not:

* run SQLite whole-table scans
* run replay validators or `quick_check`
* aggregate dashboard summaries
* await sidecar/account/control/network work
* perform direct feed-path SQLite persistence
* submit venue orders directly from feed branches

Replay-grade capture, decision evidence, live intent durability, sidecar submission, control polling, account refresh, settlement resolution, and observer summaries belong in bounded workers. Dashboard and agent freshness may degrade before bot latency degrades.

## Documentation Rules

* Use ASCII punctuation.
* Do not use em dashes or double dashes as prose separators.
* Do not use decorative separator comments.
* Do not use tables for prose.
* Use `*` for unordered Markdown list markers.
* Keep docs direct and current.
* Active implementation plans belong in the repo root only while unfinished.
* Stable docs under `docs/` describe current system truth, not chronological implementation history.
* When touching a doc, verify nontrivial claims against code/config or official docs.

Rust comment policy:

* Every Rust function needs concise `///` rustdoc, including private helpers and tests.
* Avoid inline comments inside Rust function bodies.
* Prefer names, extracted helpers, and rustdoc over explanatory comments.

TypeScript comment policy:

* Non-directive comments are rejected in dashboard TypeScript.
* Prefer naming and component structure over explanatory comments.

`make lint` and `make docs-audit` enforce these policies.

## Data Preservation

Do not create scratch DBs, WAL files, logs, screenshots, or evidence bundles in the repo root. Use `/tmp` or an ignored data path.

`runs/` is primary run evidence and must not be edited manually. `data/` is derived but still valuable. Do not delete data unless the user explicitly approves the exact files.

Before long sweeps, run:

```bash
buba-paint validate-replay-data --data <db> --start <time> --end <time>
buba-paint validate-backtest-input --data <db> --start <time> --end <time>
```

Use `prepare-backtest-input` for large sweeps. Funded live intervals also need `validate-live-fidelity`.

## Deployment Discipline

Do not improvise on `buba-paint`. Use [docs/deployment-and-ops.md](./docs/deployment-and-ops.md) and [ops/docker/Readme.md](./ops/docker/Readme.md).

Preferred remote model: Docker Compose with Caddy. Caddy is the public edge; bot, sidecar, agent, and dashboard stay private.

Minimum local gates before server work:

* `make lint`
* `make test-all`
* `cargo build --release`
* `cd dashboard/client && npm run build`

For dashboard-only or agent-only iteration, use `docker compose build <service>` and `docker compose up -d --no-deps <service>` so the bot is not disturbed.

## Debugging Pointers

* No signals: inspect `strategy_rejection_summaries` and `strategy rejection rollup` logs.
* Signal but no trade: inspect `signal_metrics.decision_status` and `rejection_reason`.
* Trades open after market close: inspect settlement and Gamma resolution logs.
* Replay mismatch: check settlement mode, reserve mode, replay quality, and backtest readiness.
* Dashboard login loops: verify dashboard JWT config, `AGENT_SECRET`, and agent health.
* Disk growth: inspect runtime DB, WAL, CLOB replay block counts, and feed class counts before deleting anything.

Useful SQL and data concepts live in [docs/data-and-replay.md](./docs/data-and-replay.md).
