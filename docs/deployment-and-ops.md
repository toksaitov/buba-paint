# Deployment And Operations

This chapter describes how Buba is run locally and remotely. The preferred remote process model is Docker Compose with Caddy TLS. Legacy systemd/manual flows are reference-only unless a fresh plan explicitly chooses them.

## Operating Posture

The normal remote mode is `live_readonly`: authenticated account and venue monitoring, replay-grade public feed capture, shadow paper trading, agent, and dashboard. It does not arm real money, place orders, cancel orders, or redeem positions.

Real-money trading is deferred. Do not infer permission to trade from credentials, sidecar write endpoints, or dashboard controls.

## Remote Layout

The production-like host is reached as `ssh buba-paint`.

Docker service shape:

* `paint`: Rust bot, run DB, feed capture, shadow paper runtime.
* `sidecar`: TypeScript Polymarket CLOB V2/authenticated venue boundary.
* `agent`: monitor-only DB/log/machine API.
* `dashboard`: authenticated dashboard backend and static frontend.
* `caddy`: public TLS edge.

Caddy publishes only `80` and `443` and reverse-proxies the dashboard. All app services stay on the private Compose network.

Host layout:

* releases: `~/buba-paint-live/releases/<timestamp>`
* active release symlink: `~/buba-paint-live/current`
* runtime state: `~/buba-paint-live/runtime/<runtime-name>`
* stable config: `~/buba-paint-live/config`
* Caddy state: `~/buba-paint-live/caddy/data` and `~/buba-paint-live/caddy/config`

## Deployment Commands

Preview:

```bash
make docker-deploy-dry-run
```

Deploy default remote `live_readonly`:

```bash
make docker-deploy
```

The Makefile calls:

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com --mode live-readonly --install-docker
```

The deploy runner stages a fresh release, verifies DNS points at the SSH host, installs Docker when requested, creates swap on small hosts when needed, uploads the sidecar env file as remote `sidecar.env`, generates dashboard/agent secrets, starts Compose, and writes a non-secret evidence bundle.

Requirements:

* `buba.toksaitov.com` resolves to the `buba-paint` host.
* inbound TCP `80` and `443` are open.
* `.secrets/buba-paint-live-sidecar.env` exists locally and remains uncommitted.
* the operator understands whether old runtime data should be preserved or deleted.

## Local Stacks

Local paper stack:

```bash
mkdir -p .docker/runtime
docker compose -f docker-compose.yml -f docker-compose.paper.yml -f docker-compose.local.yml up -d --build
```

Short local no-Caddy readonly smoke:

```bash
make live-docker-smoke
```

The smoke runner uses Docker-native runtime storage by default. That avoids macOS Docker Desktop bind-mounted SQLite WAL behavior, which is not accepted as HFT/replay evidence.

Local research control plane:

```bash
mkdir -p .docker/research/runtime .docker/research/work
docker compose -f docker-compose.research.yml up -d --build
```

This starts only `research-dashboard` and `research-worker`. It is for local orchestration testing and does not start the trading bot, sidecar, agent, Caddy, or any remote host process. The worker shares the dashboard SQLite DB and writes artifacts, prepared DBs, reports, and scratch outputs under `.docker/research/work`.

Artifact transfers are worker-owned. Same-machine transfers use append-style resumable file copies. Remote transfers use `rsync` over SSH with partial append verification, compression, and protected remote path arguments. Set `BUBA_RESEARCH_SSH_DIR` when the worker container needs a specific SSH config or key directory; the Compose file mounts it at `/home/buba/.ssh`. Running transfer rows older than `BUBA_RESEARCH_TRANSFER_STALE_MS` are recovered to `retryable` on the next worker tick so a restarted worker can resume from partial files. The default is `1800000` milliseconds; set it to `0` to disable automatic stale recovery.

Remote source artifacts enter the dashboard through `POST /api/research/artifacts/register`. That endpoint stores manifest metadata and source paths without local file reads; the research worker verifies actual bytes and checksums after transfer. Local artifacts already present under `BUBA_RESEARCH_WORK_ROOT` use `POST /api/research/artifacts/import`, which verifies files before storing the row.

Inventory dry-run plans:

```bash
python3 scripts/deploy-machine.py --machine live --dry-run
python3 scripts/deploy-machine.py --machine research --dry-run
```

Remote research stack on `testing` through Ubuntu WSL:

```bash
gh auth refresh -s write:packages
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research
```

The publisher builds `research-dashboard` and `research-worker`, pushes private GHCR images, resolves their digests, and writes `ops/research-images.lock.json`. The deploy runner syncs only the research Compose file to `/home/testing/buba-paint-research`, preserves remote `.env` and `.docker/research`, authenticates to GHCR with a temporary Docker config using the current `gh` token, pulls the digest-pinned images, removes the temporary auth config, and starts `research-dashboard` and `research-worker` without building on `testing`. The generated admin password, research worker token, and JWT secret remain on `testing` under `/home/testing/buba-paint-research/.env` and `.docker/research/config/dashboard.toml`.

The GHCR token is not persisted on `testing`. If publishing or deployment reports missing package scopes, refresh local GitHub auth with:

```bash
gh auth refresh -s write:packages
```

The generated research `.env` sets `BUBA_RESEARCH_SSH_DIR=/home/testing/.ssh` and `BUBA_RESEARCH_TRANSFER_STALE_MS=1800000`. That lets the worker container use the WSL SSH alias `buba-paint` for live-to-research artifact transfers and recover killed transfer workers after the stale window. The worker image includes `rsync` and `openssh-client`.

The worker heartbeat path is token-authenticated with `BUBA_RESEARCH_WORKER_TOKEN`. In the temporary standalone research stack, `BUBA_RESEARCH_CONTROLLER_URL` points to `http://research-dashboard:3001`, so the worker updates the local research dashboard machine row. For the final central-control deployment, point `BUBA_RESEARCH_CONTROLLER_URL` at the main dashboard URL and configure the same `BUBA_RESEARCH_WORKER_TOKEN` on that dashboard.

Remote research checks:

```bash
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'curl -sf http://localhost:3002/health'"
ssh testing "curl.exe -s http://localhost:3002/health"
```

Manual local bot run:

```bash
cargo run -p buba-paint --release -- init-db --db-path /tmp/paint.db --balance 100
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 100
```

Use `/tmp` for manual scratch DBs and logs.

## Safe Dashboard And Agent Iteration

UI or agent work should not disturb a running bot unless the change requires it. For dashboard-only or agent-only work, stage the changed release, rebuild the changed service, and restart it with `--no-deps`.

Check bot start time before and after:

```bash
ssh buba-paint 'sudo docker inspect -f "{{.State.StartedAt}}" buba-paint-paint-1'
```

Dashboard-only restart:

```bash
ssh buba-paint '
set -euo pipefail
cd ~/buba-paint-live/current
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml build dashboard
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml up -d --no-deps dashboard
'
```

Agent-only restart:

```bash
ssh buba-paint '
set -euo pipefail
cd ~/buba-paint-live/current
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml build agent
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml up -d --no-deps agent
'
```

Do not run full `docker compose down` for dashboard-only polish.

## Acceptance Checks

After deploy or restart:

```bash
ssh buba-paint 'cd ~/buba-paint-live/current && sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml ps'
curl -I http://buba.toksaitov.com
curl -fsS https://buba.toksaitov.com/health
```

Check:

* `current` points at the intended release.
* Caddy redirects HTTP to HTTPS and serves a valid certificate.
* sidecar, bot, agent, dashboard, and Caddy are running or healthy as expected.
* bot logs show `EXECUTION_MODE=live_readonly`, `FEED_EVENT_STORAGE_PROFILE=replay_grade`, latency enabled, calm/spread disabled.
* bot logs show no runtime replay-validator scans.
* DB integrity is checked only during explicit diagnostics or closeout, not healthchecks.
* live order intent, venue order, cancel, redemption, and arming tables remain empty in readonly runs.

## No-Order Readonly Soak

Before any future funded work, run a no-order host soak from a reviewed release.

Local gate:

```bash
make live-readiness-local
```

Host runner:

```bash
make live-readiness-host-soak
```

Dry-run the host plan:

```bash
make live-readiness-host-soak LIVE_HOST_SOAK_ARGS="--dry-run"
```

The soak must use:

```bash
EXECUTION_MODE=live_readonly
FEED_EVENT_STORAGE_PROFILE=replay_grade
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

It is accepted only when sidecar health is sane, host geoblock passes, account/preflight/activity checks are understood, replay validation reports `sweep_grade`, backtest-input validation reports `backtest_ready`, DB quick check passes after shutdown, no unintended live rows exist, and no stale process from an unintended release remains.

## Cleanup

Remote cleanup should be deliberate:

* stop the intended Compose project
* preserve `~/buba-paint-live/config`
* preserve `~/buba-paint-live/caddy` unless certificate state is intentionally reset
* delete only selected old runtime and release directories
* prune Docker build cache when disk pressure requires it
* copy back DB/log evidence before deleting a run that matters

Do not delete local `runs/` or valuable `data/` artifacts as part of remote server cleanup.

## Legacy Systemd

Systemd templates under [ops/](../ops/) are retained as reference material. They are not the preferred process model for new readonly or future live-readiness runs. Use Docker/Caddy unless a new operator-approved plan chooses otherwise.
