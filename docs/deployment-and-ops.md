# Deployment and Operations

This document describes durable local and remote operations guidance.

## Local Runtime

Use `/tmp` for local DBs and logs when testing manually:

```bash
cargo run -p buba-paint --release -- init-db --db-path /tmp/paint.db --balance 200
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 200
```

Docker Compose starts a local paper stack:

```bash
docker compose up -d
```

It does not start the Polymarket sidecar or an authenticated `live_readonly` venue monitor.

## Remote Layout

The production host is `buba-paint`. Do not edit code ad hoc on the server.

Remote layout:

- releases: `~/buba-paint-live/releases/<timestamp>`
- active release symlink: `~/buba-paint-live/current`
- runtime state: `~/buba-paint-live/runtime/run-0NN`
- disposable backups: `~/buba-paint-live/runtime/backups`
- archived old runs: `~/buba-paint-live/runtime/archive`
- sidecar env: `~/buba-paint-live/config/sidecar.env`
- sidecar log: `~/buba-paint-live/logs/sidecar.log`

## Process Model

Preferred process shape:

- sidecar supervised from `~/buba-paint-live/current/polymarket-sidecar`
- bot started directly, often through `script -qefa` so ANSI output lands in the run log
- agent started in `--monitor-only` mode against the bot DB
- dashboard serving static frontend files from `~/buba-paint-live/current/dashboard/client/dist`

Operations templates live under [ops/](../ops/). Use them as the starting point for supervised process work.

## Staging Flow

1. Finish code, docs, and tests locally.
2. Run local gates: `make lint`, `make test-all`, `make coverage-gate`, `cargo build --release`.
3. Build the frontend locally: `cd dashboard/client && npm run build`.
4. Stage source to a fresh remote release directory with `rsync`.
5. Exclude `.git`, `target`, `data`, `runs`, `dashboard/client/node_modules`, and old frontend build output.
6. Copy the locally built `dashboard/client/dist` into the fresh release directory.
7. Build Rust binaries on the server from the fresh release directory.

The server has historically had an older Node toolchain. Treat the local frontend build as the deployable static artifact unless the server toolchain is deliberately upgraded.

## Fresh Run

Use a fresh run for strategy changes, parameter changes, or any experiment where continuity would poison comparability.

1. Stop sidecar, bot, agent, and dashboard.
2. Verify no stale processes remain.
3. Archive or discard the old runtime according to the experiment plan.
4. Create a fresh runtime directory and DB/log paths.
5. Point `current` to the new release.
6. Start sidecar, bot, agent, and dashboard in the documented order.

## Partial Update

Use a partial update only for fixes where the current run should continue on the same DB and log, such as dashboard fixes, agent fixes, logging changes, diagnostics, or feed transport hardening that does not alter strategy semantics.

1. Back up the current run DB and log into `runtime/backups`.
2. Stop sidecar, bot, agent, and dashboard.
3. Verify no stale process from the old release remains.
4. Point `current` to the new release.
5. Restart the supervised sidecar first.
6. Restart bot, agent, and dashboard over the same runtime dir, DB, and log.
7. Verify the bot recovered the active window correctly.

Process check:

```bash
ssh buba-paint 'ps -eo pid=,args= | awk "/script -qefa|buba-paint live|buba-agent|buba-dashboard/ && !/awk/ && !/bash -c/ {print}"'
```

## Minimum Remote Acceptance

After any deploy or restart:

- `readlink -f ~/buba-paint-live/current` matches the intended release.
- `curl http://127.0.0.1:3210/health` returns a sane sidecar readiness payload.
- `curl http://127.0.0.1:9090/health` is healthy.
- `curl http://127.0.0.1:3000/health` is healthy.
- `sqlite3 ... "pragma quick_check;"` returns `ok`.
- process list shows only the intended release path.
- bot logs show sane startup and expected strategy rollups.

Before any future live-money arming, also verify host geoblock, current BTC market metadata, CLOB V2 fee/tick/min-size metadata, pUSD account diagnostics, sidecar preflight, dashboard Execution state, and current official Polymarket docs. Save the no-order readonly verification report under `data/experiments/venue-contract-v2-001/` or the current phase-specific experiment directory.

## Replay-Grade Readonly Soak

Before the first funded canary, run a no-order readonly soak on the `buba-paint` host from a reviewed release. This is not part of the local Phase 6 implementation pass; it is a later host verification gate.

Before starting the host soak, complete the local gate pack and keep its manifest:

```bash
make live-readiness-local
```

The repeatable host runner is:

```bash
make live-readiness-host-soak
```

Use `LIVE_HOST_SOAK_ARGS="--dry-run"` to inspect the plan without SSH mutations. The runner fails closed if host `live-preflight` returns `ok=false`.

Use a fresh runtime directory and keep copied-back reports under `data/experiments/replay-grade-readonly-soak-001/`. The soak must use:

```bash
EXECUTION_MODE=live_readonly
FEED_EVENT_STORAGE_PROFILE=replay_grade
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

Minimum no-order host checks:

```bash
curl -fsS http://127.0.0.1:3210/health
cargo run -p buba-paint --release -- live-preflight
sqlite3 <readonly-soak-db> 'PRAGMA quick_check;'
sqlite3 <readonly-soak-db> "SELECT key, value FROM run_metadata ORDER BY key;"
cargo run -p buba-paint --release -- validate-replay-data \
  --data <readonly-soak-db> \
  --start <soak-start-iso-time> \
  --end <soak-end-iso-time>
ssh buba-paint 'ps -eo pid=,args= | awk "/buba-paint live|buba-agent|buba-dashboard|polymarket-sidecar/ && !/awk/ {print}"'
find . -maxdepth 1 \( -name '*.db' -o -name '*.db-wal' -o -name '*.db-shm' \) -print
```

The soak is not accepted unless there is no order placement, sidecar health is sane, host geoblock passes, current BTC market metadata is captured, `validate-replay-data` reports `sweep_grade`, `PRAGMA quick_check` returns `ok`, only intended processes are running, dashboard Execution agrees with CLI preflight, and no scratch DB/WAL/SHM files appear in the repo root. If authenticated CLOB bootstrap is blocked by Cloudflare or missing L2 credentials, stop the phase and fix host/account authentication before rerunning the soak.

## Cleanup Policy

If remote disk gets tight, prune old releases, archived remote runs, and disposable remote backups. Do not delete local `runs/` or local `data/` as part of server cleanup.

Database files should stay on disk where useful, but not in Git or LFS history.

## Cross Compilation

Development is on macOS aarch64. The production host is Linux aarch64. Prefer building Rust release binaries on the server from a clean staged release unless a dedicated cross-compile workflow is being tested.
