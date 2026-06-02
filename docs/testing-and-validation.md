# Testing And Validation

This chapter describes how correctness is checked. It covers ordinary tests, low-latency guardrails, replay/backtest gates, docs fact-checking, and future live-readiness evidence.

## Standard Test Lanes

Repository-wide checks:

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

Sidecar and dashboard:

```bash
cd polymarket-sidecar && npm test && npm run build
cd dashboard/client && npm test && npm run test:e2e && npm run build
```

Use Makefile targets and package scripts as the current command source. Do not copy old command lists from chat history.

## Rust Testing

Rust unit tests live mostly under `src/*/tests/` and are included with module declarations such as `#[cfg(test)] #[path = "tests/foo_tests.rs"] mod tests;`. Integration tests live under crate-level `tests/`.

Expected coverage areas:

* strategy decisions and rejection reasons
* fee and sizing logic
* market discovery and settlement verification
* replay quality and tick replay
* backtest and sweep gates
* SQLite migrations and storage layout
* live-readonly and disarmed live-trading runtime state
* sidecar client contracts
* hot-path isolation and worker backpressure

TDD is the default. When a test fails, fix the code unless the expected value is demonstrably wrong.

## Frontend And PWA Testing

Dashboard tests use Vitest, React Testing Library, jsdom, MSW where useful, and Playwright for browser coverage.

Mobile/PWA targets:

* iPhone SE-sized viewport
* notched iPhone
* Dynamic Island-sized iPhone
* iPad Mini
* large iPad
* Pixel-class Android phone
* Android tablet-class layout
* browser mode and installed standalone simulation
* light, dark, and armed themes

The PWA must preserve safe-area spacing, keep touch targets reachable, support app-shell loading, avoid caching API/WebSocket responses, and keep notification copy honest. Current notifications are in-page/browser notifications while the dashboard is running. Full background Web Push requires a separate backend and security design.

## Sidecar Testing

The sidecar test suite covers provider bootstrap, config parsing, server routes, account/preflight health, activity recovery, user-stream handling, order validation, cancellation, redemption behavior, failure classification, and secret redaction.

Before changing venue behavior, re-check official Polymarket docs and update tests against the current contract. Do not assume old CLOB behavior is still true.

## Low-Latency Gates

The bot hot path has dedicated checks:

```bash
make hot-path-audit
make live-low-latency-local
make live-docker-smoke
```

`audit-hot-path.py` rejects known mistakes: replay validators in runtime paths, SQLite scans in decision paths, direct feed-path DB writes, sidecar/account/control awaits inside the feed reactor, legacy direct live submission paths, direct venue submissions from feed branches, opt-out tick logging defaults, and `quick_check` healthchecks.

`live-low-latency-local` writes an evidence bundle under `/tmp` by default and checks feed writer, runtime profile, and Compose guardrails.

`live-docker-smoke` runs the no-Caddy local `live_readonly` stack on Docker-native runtime storage, then checks restart counts, DB health after shutdown, feed classes, replay validation when enough data exists, and zero unintended live venue rows.

## Data Quality Gates

Public replay completeness:

```bash
buba-paint validate-replay-data --data <db> --start <time> --end <time>
```

Backtest tool compatibility:

```bash
buba-paint validate-backtest-input --data <db> --start <time> --end <time>
```

Sweep preparation:

```bash
buba-paint prepare-backtest-input --data <db> --start <time> --end <time> --output /tmp/prepared.db
```

Sweep calibration:

```bash
buba-paint sweep --data /tmp/prepared.db --start <time> --end <time> --balance <usd> --sweep PARAM=a,b --output /tmp/sweep.csv
```

When the prepared DB contains source audit rows, sweep output must include calibrated PnL columns and a calibration confidence label. Report generation should rank those rows by `calibrated_pnl` and keep raw replay `pnl` visible for audit.

Funded live evidence:

```bash
buba-paint validate-live-fidelity --db-path <db> --start <time> --end <time> --output /tmp/live-fidelity.json
```

Do not run these validators inside the trading hot path. They are offline, pre-sweep, closeout, or diagnostics tools.

## Coverage Gates

`make coverage-gate` enforces component coverage floors. Treat coverage as a regression guard, not a proof of trading profitability or venue safety.

Thin `main.rs` entrypoints are excluded from Rust coverage calculations.

## Docs Fact-Check Checklist

Every nontrivial docs claim should be traceable:

* commands and gates: `Makefile`
* Docker service shape: `docker-compose*.yml`
* deploy workflow: `scripts/deploy-docker.py`
* bot modes, defaults, storage knobs, strategy knobs, and live caps: `bots/paint/src/config.rs`
* bot CLI: `bots/paint/src/cli.rs`
* live-control behavior: `bots/paint/src/live_control.rs` and `bots/paint/src/live.rs`
* DB tables and migrations: `bots/paint/src/db/schema.rs`
* CLOB replay blocks: `bots/paint/src/db/clob_replay_blocks.rs`, `bots/paint/src/live_feed_writer.rs`, and `bots/paint/src/backtest/tick_replay.rs`
* replay/backtest gates: `bots/paint/src/backtest/` and `bots/paint/src/db/backtest_prepare.rs`
* dashboard pages: `dashboard/client/src/lib/routes.ts`
* dashboard API calls: `dashboard/client/src/lib/api.ts`
* dashboard server routes: `dashboard/server/src/main.rs`
* agent routes and machine/log behavior: `agent/src/main.rs`, `agent/src/machine.rs`, and `agent/src/process_manager.rs`
* sidecar routes/config/packages: `polymarket-sidecar/src/server.ts`, `polymarket-sidecar/src/config.ts`, and `polymarket-sidecar/package.json`
* venue facts: current official Polymarket docs, then production-safe readonly checks

If a fact cannot be traced, either remove it or phrase it as an operational assumption that must be verified.

## Documentation Hygiene

```bash
make docs-audit
make comment-audit
git diff --check
```

Docs rules:

* stable docs describe current system truth, not work history
* unfinished implementation plans are temporary root files only
* broken local links are not allowed
* root scratch files are not allowed
* ASCII punctuation only
* no prose tables unless the table is genuinely clearer than paragraphs
* no stale references to deleted prompts or plans

## Future Live-Readiness

Future funded work needs evidence, not confidence:

* local mocked sidecar, bot, dashboard, and failure-path tests
* local readiness bundle
* no-order host readonly soak
* replay-grade public capture validation
* backtest-input validation
* private live-fidelity validation for funded intervals
* dashboard Execution verification
* terminal halt and closeout export verification
* explicit operator approval before any order placement

No shortcut is acceptable because the bankroll is small.
