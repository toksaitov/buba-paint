# Commands and Configuration

This document keeps durable command and configuration guidance. Use [Readme.md](../Readme.md) for the shortest entrypoint.

## Build and Local Checks

Supported local toolchains are Rust `1.94+` and Node `22+`. Current dashboard toolchain majors are intentionally held at TypeScript `5.9`, ESLint `9`, and `@types/node` `24` until the existing config is migrated deliberately. The Rust workspace currently holds `rusqlite` at `0.33` because newer `rusqlite` releases removed direct `u64` SQL conversions used throughout the DB boundary.

```bash
cargo build
cargo build --release
cargo test
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
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
make live-readiness-local
make live-docker-smoke
```

## Local Stack

```bash
docker compose up -d
cd dashboard/client && npm run dev
```

Docker Compose starts a local paper stack with paint, agent, and dashboard. It does not start the Polymarket sidecar or authenticated `live_readonly` monitoring.

## Local Live-Money Readiness Gate

Before any host soak or funded smoke, run the local gate pack:

```bash
make live-readiness-local
```

By default, the gate writes command logs and `manifest.json` under `/tmp/buba-live-readiness-local-<timestamp>`. It refuses repo-local output directories unless explicitly overridden for debugging. For a safe runner-only check:

```bash
make live-readiness-local LIVE_READINESS_ARGS="--dry-run --output-dir /tmp/buba-readiness-dry-run"
```

The manifest records git SHA, dirty status, host metadata, selected redacted environment values, each command, each log path, exit statuses, and final pass/fail. Treat any failed command as a blocker before moving to host verification.

The short local Docker smoke runner is:

```bash
make live-docker-smoke
```

It runs the no-Caddy `live_readonly` stack for 10 minutes by default, uses a Docker-native runtime volume instead of a macOS bind mount, copies DB/log evidence to `/tmp/buba-live-docker-smoke-<timestamp>`, then runs offline DB/log checks and `validate-replay-data` when feed rows were captured. This is the local review loop for Docker runtime health. Do not treat macOS Docker Desktop bind-mounted SQLite WAL evidence as valid HFT or replay-grade smoke evidence.

The standalone runtime profiler is:

```bash
make live-runtime-profile
```

It runs the latency-only paper runtime against a `/tmp` DB, samples process CPU, captures Linux `perf` data when available, and writes evidence under `/tmp/buba-live-runtime-profile-<timestamp>`.

The host no-order soak runner is:

```bash
make live-readiness-host-soak
```

Use `LIVE_HOST_SOAK_ARGS="--dry-run"` to inspect the command plan without touching the host. The runner stages a release on `buba-paint`, uses stable config files under `~/buba-paint-live/config`, writes runtime data under a fresh `~/buba-paint-live/runtime/soak-...` directory, stops services at closeout by default, and writes non-secret local evidence under `data/experiments/replay-grade-readonly-soak-001/`. A failed `live-preflight` is a hard blocker.

## Bot Commands

```bash
cargo run -p buba-paint --release -- init-db --db-path /tmp/paint.db --balance 100
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 100
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 100 --set LATENCY_ARB_MAX_ASK=0.60
cargo run -p buba-paint --release -- live-preflight
cargo run -p buba-paint --release -- latency-probe --timeout-ms 3000
cargo run -p buba-paint --release -- db-footprint --db-path /tmp/paint.db
```

Use `/tmp` for local scratch DBs. Do not create DBs in the repository root.

## Backtest and Sweep

```bash
cargo run -p buba-paint --release -- backtest \
  --data data/market-data.db \
  --start 2026-02-20T03:13 \
  --end 2026-02-28T00:00 \
  --balance 100

cargo run -p buba-paint --release -- validate-replay-data \
  --data data/market-data.db \
  --start 2026-02-20T03:13 \
  --end 2026-02-28T00:00

cargo run -p buba-paint --release -- validate-backtest-input \
  --data data/market-data.db \
  --start 2026-02-20T03:13 \
  --end 2026-02-28T00:00

cargo run -p buba-paint --release -- prepare-backtest-input \
  --data /path/to/runtime/paint.db \
  --start 2026-05-09T00:00:00Z \
  --end 2026-05-10T00:00:00Z \
  --output /tmp/prepared-backtest.db

cargo run -p buba-paint --release -- validate-live-fidelity \
  --db-path /path/to/live-run/paint.db \
  --start 2026-05-01T00:00:00Z \
  --end 2026-05-01T01:00:00Z \
  --output /tmp/live-fidelity.json

cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20 \
  --end 2026-03-04 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012 \
  --output data/sweeps/example/sweep.csv
```

`--sweep PARAM=start:end:step` generates a range. `--sweep PARAM=a,b,c` enumerates values. `--set PARAM=value` fixes a parameter without sweeping. Boolean values accept `true/false`, `1/0`, `yes/no`, and `on/off`; operator docs should prefer `true/false`.

`validate-replay-data` proves raw public feed completeness. `validate-backtest-input` proves the current backtester can load the interval, derive missing market open/close prices, find settled outcomes, and run a bounded replay dry run. `prepare-backtest-input` creates an offline derived DB with compact CLOB replay rows and sweep indexes; use it for large sweeps instead of hitting an append-optimized runtime DB directly. Sweeps refuse inputs unless both correctness gates pass and warn when the input is backtest-ready but not prepared/indexed. If an interval contains funded `live_trading` evidence, sweeps also require `research_grade_live` from `validate-live-fidelity`. Backtests still run on descriptive archives, but they warn when the interval lacks replay-grade decision inputs.

## Settlement and Historical Data

```bash
cargo run -p buba-paint --release -- verify-settlements --db data/market-data.db --concurrency 15
cargo run -p buba-paint --release -- build-data --runs-dir runs --output data/market-data.db
cargo run -p buba-paint --release -- upgrade-history --runs-dir runs --from-run 4 --to-run 9 --rebuild-derived --output data/market-data.db
```

`verify-settlements` fetches actual Polymarket outcomes and compares them against locally derived settlements. `build-data` merges run DBs into derived data. `upgrade-history` performs additive historical upgrades and caches HTTP payloads under `data/backfill-cache/`.

## Exact-Run Replay

For exact pulled-run calibration, prefer observed resolution timing:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
cargo run -p buba-paint --release -- backtest \
  --data /tmp/run-replay-data.db \
  --start 2026-04-04T20:15 \
  --end 2026-04-08T17:25 \
  --balance 100 \
  --set LATENCY_ARB_ENABLED=true \
  --set SPREAD_CAPTURE_ENABLED=false \
  --set CALM_PERSISTENCE_ENABLED=false
```

The pending-settlement reserve defaults match the selected run-012 latency-only canary profile. Override them only when intentionally comparing compatibility or conservative modes. See [pending-settlement-modes.md](./pending-settlement-modes.md).

## Core Environment Knobs

Use [.env.example](../.env.example) as the canonical template.

Important groups:

* Execution mode: `EXECUTION_MODE=paper|live_readonly|live_trading`.
* Storage profile: `FEED_EVENT_STORAGE_PROFILE=replay_grade|compact|full_debug`.
* Feed freshness: `MAX_SIGNAL_FEED_AGE_MS`, `MAX_QUOTE_AGE_MS`, `WEBSOCKET_CONNECT_TIMEOUT_MS`, `BINANCE_NO_MESSAGE_RECONNECT_MS`, `CLOB_NO_MESSAGE_RECONNECT_MS`.
* Pending settlement: `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION`, `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION`, `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION`, `BACKTEST_SETTLEMENT_MODE`.
* Live caps: `LIVE_SESSION_CASH_CAP_USD`, `LIVE_MAX_SINGLE_ORDER_USD`, `LIVE_MAX_OPEN_NOTIONAL_USD`, `LIVE_MAX_DAILY_LOSS_USD`, `LIVE_MAX_SESSION_DRAWDOWN_USD`, `LIVE_MIN_REQUIRED_CASH_USD`. The open-exposure ceiling `min(LIVE_MAX_OPEN_NOTIONAL_USD, LIVE_SESSION_CASH_CAP_USD)` is enforced continuously at capital reservation in the shared bankroll, so total committed open exposure cannot exceed it regardless of mid-session balance drift.
* Backtest fidelity: `ENFORCE_OPEN_EXPOSURE_CAPS` (default false) makes paper and backtest runs apply the same open-exposure ceiling the live path enforces, so a live-fidelity backtest models the caps. It is always active in live execution modes; leave it off for research sweeps, which then size exactly as before.
* Live identity guards: `LIVE_EXPECTED_SIGNATURE_TYPE` (optional exact pin, one of `0`, `1`, `2`, `3`), `LIVE_ALLOW_DEPOSIT_WALLET` (default false; only with `LIVE_EXPECTED_SIGNATURE_TYPE=3` does it permit a deposit-wallet signature type), `LIVE_EXPECTED_EGRESS_IP` (optional egress-IP pin checked against the sidecar geoblock IP).
* Live on-chain reconciliation: `LIVE_ONCHAIN_RECONCILE` (default true) is an operator kill-switch for post-fill on-chain CTF balance verification, which runs only in live execution and never in paper or backtest. `LIVE_ONCHAIN_RECONCILE_GRACE_MS` (default 6000), `LIVE_ONCHAIN_RECONCILE_RETRY_INTERVAL_MS` (default 3000), and `LIVE_ONCHAIN_RECONCILE_MAX_ATTEMPTS` (default 5) bound the settlement grace and the bounded retry budget. The sidecar reads the chain through `POLYMARKET_POLYGON_RPC_URL` (default `https://polygon-rpc.com`), which is privileged operator config.
* Worker and storage budgets: `FEED_EVENT_WRITER_QUEUE_CAPACITY`, `FEED_EVENT_WRITER_BATCH_SIZE`, `FEED_EVENT_WRITER_FLUSH_MS`, `FEED_EVENT_WRITER_MAX_LAG_MS`, `CLOB_REPLAY_BLOCK_MAX_ROWS`, `CLOB_REPLAY_BLOCK_MAX_MS`, `CLOB_REPLAY_BLOCK_ZSTD_LEVEL`, `LIVE_RUNTIME_MAX_DB_BYTES`, `LIVE_FEED_BATCH_MAX_MESSAGES`, `LIVE_DECISION_QUEUE_CAPACITY`, `LIVE_DECISION_OUTPUT_QUEUE_CAPACITY`, `LIVE_RUNTIME_PERSISTENCE_QUEUE_CAPACITY`, `LIVE_SUBMISSION_QUEUE_CAPACITY`, `MAX_LIVE_DECISION_AGE_MS`, `WORKER_SHUTDOWN_TIMEOUT_MS`.
* Strategy toggles: `LATENCY_ARB_ENABLED`, `SPREAD_CAPTURE_ENABLED`, `CALM_PERSISTENCE_ENABLED`.
* Sidecar: `LIVE_SIDECAR_URL`, `POLYMARKET_PRIVATE_KEY`, `POLYMARKET_PROXY_WALLET`, `POLYMARKET_FUNDER`, `POLYMARKET_RELAYER_HOST`, `POLYMARKET_POLYGON_RPC_URL`, `POLYMARKET_EXPECTED_TAKER_FEE_RATE`, `POLYMARKET_USER_STREAM_STALENESS_MS`, `POLYMARKET_RELAYER_API_KEY`, `POLYMARKET_RELAYER_API_KEY_ADDRESS`, `POLYMARKET_BUILDER_API_KEY`, `POLYMARKET_BUILDER_SECRET`, `POLYMARKET_BUILDER_PASSPHRASE`.

The sidecar CLOB boundary uses `@polymarket/clob-client-v2@1.0.6`, pUSD collateral diagnostics, and proxy-wallet signature type `1` for the first account model. `POLYMARKET_FUNDER` defaults to `POLYMARKET_PROXY_WALLET` when omitted. Optional CLOB L2 credentials may be configured with `POLYMARKET_API_KEY`, `POLYMARKET_API_SECRET`, and `POLYMARKET_API_PASSPHRASE`; otherwise the sidecar derives or creates them through authenticated CLOB L1 bootstrap. Gasless redemption uses `@polymarket/builder-relayer-client@0.0.10` with `@polymarket/builder-signing-sdk@1.0.0` and stays fail-closed unless the configured credentials are complete. The builder-signing `BuilderConfig` is constructed only for the gasless `RelayClient` redemption path and is intentionally retained for that reason; the V2 order-submission path does not use it for order attribution. Preflight reads the live per-market crypto taker rate (`fd.r`) and logs a `live_fee_rate_mismatch` warning and a `fee_rate_mismatches` preflight detail when it differs from `POLYMARKET_EXPECTED_TAKER_FEE_RATE` (default `0.07`, matching the configured bot `taker_fee_rate`); the mismatch is observability only and never blocks arming. The bot fee constant `taker_fee_rate` is `0.07`, the current crypto taker rate; any further change to it is a numeric-sensitive operator decision. Bulk cancellation uses `cancelAll` with no client-supplied order-id list and the fill-or-kill strategy never rests more than a handful of open orders, so the venue 1000-id bulk-cancel limit does not apply. Market discovery and resolution fetch Gamma by event slug rather than a `closed`-filtered list query, so the Gamma `closed` default does not affect them, and the neg-risk flag is taken from CLOB V2 metadata rather than the Gamma filter. The authenticated user stream runs an inactivity watchdog: the heartbeat sends a `PING` every ten seconds and, if no inbound frame (such as the venue `PONG`) arrives within `POLYMARKET_USER_STREAM_STALENESS_MS` (default `45000`, floored at two ping intervals, `0` disables), it forces a reconnect, guarding against a socket that stays open but silently stops delivering data. Set the window comfortably above several ping intervals so a normally quiet stream is not reconnected during idle periods. The preflight clock-drift check surfaces the last observed drift as `last_clock_drift_ms` in health; hosts running the sidecar must keep their clock NTP-synced, since drift causes intermittent invalid-signature order rejections. At startup the sidecar asserts the SDK's resolved V2 exchange contract addresses (`exchangeV2`, `negRiskExchangeV2`) against pinned expected values and fails loudly on a mismatch. This is a guard, not a complete ExchangeV3 detector: it catches an SDK address repoint of the V2 exchanges, but a venue V3 cutover that keeps the V2 constants while changing routing or domain semantics still requires watching the client changelog. Arming fails closed when the sidecar reports an unknown signature type, when it reports signature type `3` (deposit wallet) unless both `LIVE_ALLOW_DEPOSIT_WALLET` and `LIVE_EXPECTED_SIGNATURE_TYPE=3` are set, and when a configured `LIVE_EXPECTED_EGRESS_IP` does not match the observed geoblock IP. Arming also refuses when the live-order idempotency index is absent; the runtime records a durable per-intent venue-attempt marker before each submission so a retried or restarted submission cannot place a second real order for one intent, and a partial fill on a fill-or-kill order is treated as a blocking anomaly rather than a normal fill. Preflight fails closed when a contract-based trading wallet (signature type `1` or `2`) has no on-chain code, since an undeployed proxy or Safe cannot settle or redeem. After each live fill a detached bounded worker reads the on-chain CTF balance for the filled token through the sidecar and raises a critical reconciliation event, which the next remote-state refresh treats as a terminal halt, when the balance is short of the filled size or cannot be verified within the retry budget.

`live_trading` starts disarmed and is local-verification only. Do not treat the presence of live caps, sidecar credentials, callable sidecar write endpoints, or queued `live-control` commands as permission to deploy real money. Future funded trading requires a fresh operator-approved plan.

The dashboard `Parameters` page (route `/parameters`, agent endpoint `GET /api/runtime/config`, dashboard proxy `GET /api/bots/:id/config`) renders the sanitized snapshot the bot writes once at startup under `run_metadata.runtime_config_snapshot`. It is read-only; values cannot be edited from here. Use it to confirm what knobs the deployed runtime is actually using without SSH. The snapshot covers execution mode, storage profile, strategy toggles and per-strategy parameters, risk and live-cap knobs, pending-settlement reserves, fee assumptions, feed freshness gates, and worker budgets. Secrets are never serialized.

The dashboard `Machine` page (route `/machine`, agent endpoint `GET /api/machine`, dashboard proxy `GET /api/bots/:id/machine`) renders host CPU (global plus per-core), memory, swap, disk, and runtime DB / WAL / SHM file sizes with a 5-minute timeline per metric. The agent samples cross-platform metrics via the `sysinfo` crate every 5 s on a dedicated thread; samples live in an in-memory ring buffer and are not persisted to SQLite. Runtime DB file sizes are stat'd per-request, so the bot SQLite is never opened. Inside Docker the page is agent-container-scoped (labeled "agent host view" on the Disk card); load average is `null` on Windows; iowait is not surfaced. The page replaces several `ssh + df / free / top` ad-hoc commands.

## Live Control CLI

The local CLI queues the same audited bot-applied commands as the dashboard Execution controls:

```bash
cargo run -p buba-paint --release -- live-control \
  --db-path /tmp/paint.db \
  arm \
  --actor operator \
  --reason "preflight gates passed"
```

Supported actions are `preflight`, `arm`, `disarm`, `stop-after-flat`, `kill-switch`, `cancel-all`, and `redeem-all`. Commands are written into the bot DB and applied by a running `EXECUTION_MODE=live_trading` process. The command is rejected if there is no active live-trading session.

The dashboard route `POST /api/bots/:id/live/control` proxies to the agent route `POST /api/live/control`. Only dashboard admins may submit it. The server injects the authenticated actor, and the bot remains the only process that applies controls or touches the sidecar.

## Live Closeout CLI

Terminal live sessions require an evidence package before a new funded run DB is started:

```bash
cargo run -p buba-paint --release -- live-closeout \
  --db-path /path/to/live-run/paint.db \
  --output-dir /path/to/closeout \
  --actor operator \
  --reason "session drawdown halt"
```

`live-closeout` writes `summary.json`, `manifest.json`, `db_integrity.txt`, `replay_quality.txt`, `live_fidelity.txt`, live ledger exports, control audit, and a `postmortem.md` stub. It records `live_closeout_exported` in the DB audit ledger. It does not make a halted DB re-armable; the next funded attempt must use a new run DB.

The closeout summary and manifest include observed replay-quality class, live-fidelity class, validation intervals, missing public feed classes, and missing private live requirements. If the interval is not `sweep_grade` or the funded run is not `research_grade_live`, the postmortem stub labels the run descriptive-only.

Live-money risk defaults are:

* `LIVE_MAX_DAILY_LOSS_USD=15`
* `LIVE_MAX_SESSION_DRAWDOWN_USD=20`
* existing `MAX_DRAWDOWN_PCT`

An armed live session treats unresolved unknown order state, critical reconciliation, auth/geoblock/storage failure, or persistent account/user-stream/venue degradation as terminal blockers. `cancel-all` and `redeem-all` may still be queued for cleanup when their capability data says they are safe.

## Strategy Defaults

The current candidate settings live in [.env.example](../.env.example). Do not promote historical run settings without fresh replay-grade evidence.

For any future first funded canary, code should support all strategy families, but runtime config should enable latency only:

```bash
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

That policy is the documented future funded baseline, not current remote behavior. Current remote operation remains `live_readonly` unless a fresh funded-run plan explicitly changes it.
