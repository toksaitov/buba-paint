# Testing and Validation

This document describes durable validation practices for the workspace.

## Standard Gates

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test
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
```

Sidecar and dashboard commands:

```bash
cd polymarket-sidecar && npm test
cd polymarket-sidecar && npm run build
cd polymarket-sidecar && npm run audit:security
cd dashboard/client && npm test
cd dashboard/client && npm run test:e2e
cd dashboard/client && npm run test:coverage
cd dashboard/client && npm run build
```

Test inventory changes frequently. Prefer the Makefile and package scripts over hard-coded test counts.

## Rust Tests

Rust unit tests usually live under `src/*/tests/` and are included with `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`. Integration tests live under crate-level `tests/`.

Coverage emphasis:

- paint bot: replay, execution, fees, accounting, migrations, feeds, strategy logic, and live-system behavior.
- agent: REST handlers, WebSocket polling/broadcast, DB reader compatibility, process manager, auth, and integration round-trips.
- dashboard server: auth/JWT, bot proxy handlers, WebSocket proxy, config parsing, user management, error mapping, and degraded proxy behavior.

TDD is expected. When a test fails, fix the code, not the test, unless the expected value is provably wrong and the change is documented.

## Frontend and E2E Tests

Dashboard client tests use Vitest, React Testing Library, jsdom, and MSW where useful. Setup lives under `dashboard/client/src/test/`.

Playwright lives under `dashboard/client/e2e/` and covers desktop and mobile viewport behavior. Mobile coverage is important because the dashboard is used as an operator surface, but mobile must not become the primary arming surface for future real-money controls.

## Sidecar Tests

The TypeScript sidecar tests cover provider behavior, lifecycle, server routes, auth/account/preflight handling, user-stream resilience, FOK/FAK order submission validation, cancellation, redemption state handling, and failure classification. The sidecar security audit gate fails on moderate-or-higher npm advisories; current upstream Polymarket SDK dependencies still carry low-severity ethers/elliptic advisories with no available fix.

Before live-money work touches the sidecar or bot runtime, re-check current official Polymarket docs and update tests to the current venue contract. The sidecar write boundary is implemented, but the bot `live_trading` runtime and dashboard arming controls remain gated until later phases.

## Comment and Docs Policy

`make lint` runs strict Rust and TypeScript comment audits.

Rust policy:

- Every Rust function needs concise `///` rustdoc, including private helpers and tests.
- Avoid inline comments inside Rust function bodies.
- Prefer self-explanatory names, extracted helpers, and rustdoc.
- Do not use decorative separator comments.

TypeScript policy:

- Non-directive comments are rejected in dashboard TypeScript.
- Prefer names and component structure over explanatory comments.

Markdown policy:

- Use ASCII punctuation.
- Do not use em dashes or double dashes as prose separators.
- Do not use tables for prose.
- Keep docs direct and current.
- Active implementation plans belong in the repo root, not in `docs/`.

## Docs Audit

```bash
make docs-audit
```

The docs audit checks readme casing, local Markdown links, stale layout references, data directory notes, transient SQLite files under derived data, root scratch files, and active-plan files under `docs/`.

## Coverage Gates

Coverage floors are enforced by `make coverage-gate`. Thin `main.rs` bootstrapping entrypoints are excluded from Rust coverage calculations.

Current floor targets:

- `buba-paint`: 80%
- `buba-agent`: 90%
- `buba-dashboard`: 90%
- dashboard frontend: 80%

Treat coverage as a regression guard, not as proof of trading safety.

## Real-Money Readiness Validation

Before any funded run, the validation ladder in [LIVE_TRADING_PLAN.md](../LIVE_TRADING_PLAN.md) controls. At minimum, live-money work requires:

- local mocked tests for sidecar, bot, dashboard, and failure paths
- readonly production-host smoke checks
- replay-grade data validation
- dashboard Execution state verification
- explicit operator approval before any order-placement smoke
- post-run DB/log/account export and postmortem

No validation shortcut is acceptable just because the bankroll is small.
