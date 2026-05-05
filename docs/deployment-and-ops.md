# Deployment and Operations

This document describes durable local and remote operations guidance.

## Local Runtime

Use `/tmp` for local DBs and logs when testing manually:

```bash
cargo run -p buba-paint --release -- init-db --db-path /tmp/paint.db --balance 100
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 100
```

Docker Compose starts a local paper stack:

```bash
mkdir -p .docker/runtime
docker compose -f docker-compose.yml -f docker-compose.paper.yml -f docker-compose.local.yml up -d --build
```

The local paper stack does not start the Polymarket sidecar or an authenticated `live_readonly` venue monitor.

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

Preferred remote process shape is Docker Compose with Caddy as the only public edge:

- Caddy publishes ports `80` and `443`, provisions certificates, and reverse-proxies the dashboard.
- dashboard, agent, bot, and sidecar stay on a private Docker network.
- runtime DBs and logs are host bind mounts under `~/buba-paint-live/runtime/<runtime-name>`.
- Caddy state is persisted under `~/buba-paint-live/caddy`.

Systemd templates under [ops/](../ops/) are retained as legacy/reference artifacts. Use Docker Compose for new deployments unless a phase plan explicitly says otherwise.

## Docker/Caddy Staging Flow

The repeatable Docker deployment command is:

```bash
make docker-deploy
```

The default target is equivalent to:

```bash
python3 scripts/deploy-docker.py \
  --host buba-paint \
  --domain buba.toksaitov.com \
  --mode live-readonly \
  --install-docker
```

Use `make docker-deploy-dry-run` before mutating the host. The runner stages a fresh release, generates dashboard/agent secrets, uploads `.secrets/buba-paint-live-sidecar.env` to the host as `sidecar.env`, starts Compose, and writes a non-secret evidence bundle under `data/experiments/docker-deploy-*`.

The production stack uses:

- `docker-compose.yml` for shared internal services.
- `docker-compose.live-readonly.yml` for authenticated readonly monitoring.
- `docker-compose.paper.yml` for paper-only runs.
- `docker-compose.prod.yml` for Caddy TLS and host bind mounts.

`buba.toksaitov.com` must resolve to the same host reached by `ssh buba-paint`, and the cloud firewall must allow inbound TCP `80` and `443` for Caddy certificate provisioning. The deploy runner checks DNS before staging because Caddy cannot complete ACME validation when the A record points at an old instance. On small hosts the runner also enables a 4 GiB swap file by default when no swap is active, which keeps Docker image builds from being killed by memory pressure.

## Legacy Staging Flow

1. Finish code, docs, and tests locally.
2. Run local gates: `make lint`, `make test-all`, `make coverage-gate`, `cargo build --release`.
3. Build the frontend locally: `cd dashboard/client && npm run build`.
4. Stage source to a fresh remote release directory with `rsync`.
5. Exclude `.git`, `target`, `data`, `runs`, `dashboard/client/node_modules`, and old frontend build output.
6. Copy the locally built `dashboard/client/dist` into the fresh release directory.
7. Build Rust binaries on the server from the fresh release directory.

The server has historically had an older Node toolchain. Treat the local frontend build as the deployable static artifact unless the server toolchain is deliberately upgraded.

Use this legacy flow only when Docker is explicitly out of scope.

## Fresh Run

Use a fresh run for strategy changes, parameter changes, or any experiment where continuity would poison comparability.

1. Stop the Docker Compose project or legacy sidecar, bot, agent, and dashboard.
2. Verify no stale processes remain.
3. Archive or discard the old runtime according to the experiment plan.
4. Create a fresh runtime directory and DB/log paths.
5. Point `current` to the new release.
6. Start the Docker stack or sidecar, bot, agent, and dashboard in the documented order.

## Partial Update

Use a partial update only for fixes where the current run should continue on the same DB and log, such as dashboard fixes, agent fixes, logging changes, diagnostics, or feed transport hardening that does not alter strategy semantics.

1. Back up the current run DB and log into `runtime/backups`.
2. Stop the Docker stack or sidecar, bot, agent, and dashboard.
3. Verify no stale process from the old release remains.
4. Point `current` to the new release.
5. Restart the Docker stack, or restart the supervised sidecar first in the legacy process model.
6. Restart bot, agent, and dashboard over the same runtime dir, DB, and log if not using Compose.
7. Verify the bot recovered the active window correctly.

Process check:

```bash
ssh buba-paint 'cd ~/buba-paint-live/current && sudo docker compose ps'
```

## Minimum Remote Acceptance

After any deploy or restart:

- `readlink -f ~/buba-paint-live/current` matches the intended release.
- `curl -I http://buba.toksaitov.com` redirects to HTTPS.
- `curl https://buba.toksaitov.com/health` is healthy with a valid certificate.
- internal sidecar, agent, and dashboard health checks pass through `docker compose exec`.
- `sqlite3 ... "pragma quick_check;"` returns `ok`.
- `docker compose ps` shows only the intended project services.
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
