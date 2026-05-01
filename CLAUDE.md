# CLAUDE.md

AI development guidelines for the buba workspace. [AGENTS.md](./AGENTS.md) is only a compatibility alias. Treat this file as the canonical repository instruction file.

## Project

`buba` is a Rust-first trading research workspace for Polymarket prediction markets. The first bot, `paint`, trades 5-minute BTC Up/Down markets in paper mode, supports authenticated `live_readonly` venue monitoring, and has a local disarmed `live_trading` runtime for mocked verification.

Primary components:

- `bots/paint`: bot runtime, strategies, feeds, SQLite persistence, backtests, sweeps, settlement verification, and replay-data validation.
- `agent`: read-only DB monitor with REST and WebSocket APIs.
- `dashboard/server`: authenticated dashboard backend and agent proxy.
- `dashboard/client`: React dashboard.
- `polymarket-sidecar`: TypeScript authenticated Polymarket boundary for readonly checks, FOK/FAK order submission, cancellation, and redemption.

Durable system docs start at [docs/Readme.md](./docs/Readme.md). Active unfinished live-money work lives in [LIVE_TRADING_PLAN.md](./LIVE_TRADING_PLAN.md), not in `docs/`.

Current safety state:

- `paper` is the production research mode.
- `live_readonly` is real authenticated venue/account monitoring plus shadow paper trading.
- `live_trading` starts disarmed, requires audited live-control commands from the CLI or admin dashboard, and is not deployed or armed.
- Sidecar `/health`, `/account`, `/preflight`, `/orders`, `/cancel`, `/cancel-all`, and `/redeem-all` are real venue-boundary surfaces.
- Bot runtime order submission, live ledger persistence, reconciliation, CLI control, and admin dashboard control queueing are local-verification surfaces only. Deployment and real arming remain unfinished phases.

## Build and Test

```bash
cargo build
cargo build --release
cargo test
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
make lint
make comment-audit
make docs-audit
make test-fast
make test-integration
make test-slow
make test-e2e
make test-all
make coverage
make coverage-gate

cd dashboard/client && npm test
cd dashboard/client && npm run test:e2e
cd dashboard/client && npm run test:coverage
cd polymarket-sidecar && npm test
cd polymarket-sidecar && npm run build
```

Use [docs/commands-and-config.md](./docs/commands-and-config.md) for command details.

## Architecture Rules

1. No `unwrap()` or `expect()` in library code. Use `anyhow::Result` or `thiserror`. Acceptable only in tests and in `main()`.
2. Use `f64`, never `f32`, for prices, balances, and fractions.
3. Config is immutable after construction. Pass `&Config`.
4. Clock is injectable through the `Clock` trait.
5. Database layer owns SQL. Do not add raw SQL outside `src/db/`.
6. Strategies are stateful structs implementing the `Strategy` trait.
7. Unit tests live under `src/*/tests/` via `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`. Integration tests live under crate-level `tests/`.
8. TDD is required. When a test fails, fix the code, not the test, unless the expected value is demonstrably wrong.
9. Frontend tests use Vitest and React Testing Library, colocated in `__tests__/` directories.
10. After major behavior, schema, API, or workflow changes, revise relevant docs. Do not append blindly.

## Documentation Style

When writing Markdown, comments, or prose:

- Use ASCII punctuation. Do not use em dashes or double dashes as prose separators.
- Do not overuse bold. Markdown must be readable as plain text.
- Do not use ASCII art diagrams.
- Do not use tables for prose.
- Do not hard-wrap prose.
- Keep writing direct and current.
- Every Rust function needs concise `///` rustdoc, including private helpers and tests.
- Do not leave inline comments inside Rust function bodies.
- Do not use decorative separator comments.
- When touching a file, remove or rewrite stale comments.
- Active implementation plans belong in the repo root, not in `docs/`.

`make lint` enforces strict Rust and TypeScript comment policies. `make docs-audit` enforces documentation hygiene.

## Module Map

Paint bot:

- `cli.rs`: CLI parsing and command dispatch.
- `live.rs`: shared paper, readonly, and disarmed live-trading runtime.
- `live_readonly.rs`: readonly sessions, preflight, account snapshots, reconciliation, and rollups.
- `config.rs`: env config, sweep overrides, execution mode, caps, reserve knobs, and settlement mode.
- `feeds/`: Binance, CLOB, Chainlink, and feed utility code.
- `strategies/`: latency-arb, spread-capture, and calm-persistence.
- `strategy_cycle.rs`: shared evaluation, routing, signal persistence, and submission.
- `executor.rs`: shared paper execution model.
- `signal_features.rs`: shared feature engine.
- `bankroll.rs`, `position_manager.rs`, `portfolio.rs`: sizing, trade lifecycle, reserve handling, and regime routing.
- `market_discovery.rs`, `fees.rs`, `verify.rs`: venue metadata, dynamic fees, and settlement verification.
- `backtest/`: replay, feed state, window manager, runner, and sweeps.
- `db/`: schema, database wrapper, historical upgrades, and derived-data build tools.
- `live_sidecar.rs`: typed Rust client for the local sidecar.

Agent:

- `api.rs`, `db_reader.rs`, `ws.rs`, `process_manager.rs`, `auth.rs`, `types.rs`, `error.rs`.

Dashboard server:

- `auth.rs`, `config.rs`, `db.rs`, `proxy.rs`, `error.rs`, and `api/*`.

Dashboard client:

- `pages/`: Login, Overview, Execution, Logs, Equity, Trades, Signals, Strategies.
- `hooks/`: API/cache hooks.
- `lib/`: API client, routes, types, formatting, WebSocket, trading-summary presentation helpers.
- `components/`: layout, common, UI primitives, charts, tables, and dashboard surfaces.
- `stores/`: auth, mobile nav, and theme state.

Sidecar:

- TypeScript package for proxy-wallet auth, account/preflight checks, user-stream health, and future real order/redeem boundary.

Use [docs/system-architecture.md](./docs/system-architecture.md) for the durable architecture narrative.

## Data Preservation

`runs/` contains primary run data and must not be edited manually. `data/` contains derived data and should still be treated as valuable. Do not create temporary or test databases in the project root. Use `/tmp` or test tempfiles.

Replay-grade capture is the research default. Run `buba-paint validate-replay-data` before long sweeps. See [docs/data-and-replay.md](./docs/data-and-replay.md).

## Deployment Discipline

Do not improvise on the `buba-paint` server. Use the release-directory and runtime-directory workflow documented in [docs/deployment-and-ops.md](./docs/deployment-and-ops.md).

Minimum local gates before server work:

- `make lint`
- `make test-all`
- `make coverage-gate`
- `cargo build --release`
- `cd dashboard/client && npm run build`

The server may not have the right Node version for frontend builds. Prefer building `dashboard/client/dist` locally and copying it into the staged release.

## Key Behavioral Constraints

- Raw `feed_events` replay at exact timestamps. Legacy `tick_data` fallback is lower fidelity.
- Conservative pending-settlement reserve mode is the default. See [docs/pending-settlement-modes.md](./docs/pending-settlement-modes.md).
- Settlement can record provisional observability, but bankroll and strategy state update only on authoritative Polymarket outcomes.
- Market windows activate only after their start time.
- Spread legs are independent. One-sided residual exposure is possible.
- Dynamic taker fees must not be hardcoded without checking current venue metadata.
- Feed reconnect knobs reduce stale downtime only. They must not loosen stale-data gates.
- CLOB freshness uses local observed receipt time when source timestamps are missing, while preserving raw source timestamps for replay/debugging.
- Missing CLOB size fields must not overwrite existing in-memory liquidity with zero.
- Submit-time sizing must enforce min bet and venue min size before queueing.
- `SIM_ORDER_LATENCY_MS` is a paper assumption, not measured venue latency.
- `AGENT_SECRET` is required for normal agent startup.
- Exact pulled-run replay should use `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`.
- Old runs without replay-grade Binance book state are descriptive-only for parameter selection.

## Common Debugging Pointers

- No signals: inspect `strategy_rejection_summaries` and `strategy rejection rollup` logs before retuning.
- Signal but no trade: inspect `signal_metrics.decision_status` and `rejection_reason`.
- Trades open after market close: inspect pending-resolution and Gamma settlement logs.
- Replay mismatch: check settlement mode, reserve mode, and replay data quality.
- Dashboard login loops: verify `AGENT_SECRET`, dashboard JWT config, and agent health.
- Root DB garbage: move scratch DBs to `/tmp`.

Useful SQL snippets live in [docs/data-and-replay.md](./docs/data-and-replay.md).

## Naming and Precision

- Rust files and functions: snake_case.
- Rust types: PascalCase.
- Frontend files: kebab-case.
- Frontend components: PascalCase.
- Config fields: snake_case.
- Sweep CSV output: raw `f64`, no rounding.
