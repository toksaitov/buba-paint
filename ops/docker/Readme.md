# Docker Deployment

Docker Compose with Caddy is the preferred single-host deployment model.

## Requirements

* DNS: `buba.toksaitov.com` points at the `buba-paint` host.
* Firewall: inbound TCP `80` and `443` are open for Caddy and Let's Encrypt.
* SSH: `ssh buba-paint` works from the operator machine.
* Local secrets: `.secrets/buba-paint-live-sidecar.env` exists and is not committed.
* Host capacity: small AWS hosts need swap for image builds. The deploy runner creates a 4 GiB swap file by default when no swap is active.

## One-Command Deploy

Preview the plan:

```bash
make docker-deploy-dry-run
```

Deploy the default `live_readonly` stack:

```bash
make docker-deploy
```

The runner installs Docker if needed, stages a fresh release, writes config under `~/buba-paint-live/config`, writes runtime DB/logs under `~/buba-paint-live/runtime/<runtime-name>`, starts Caddy, and writes a non-secret evidence bundle under `data/experiments/docker-deploy-*`.

The runner refuses production TLS deployment if the domain A record does not resolve to the SSH host. Fix DNS before retrying; Caddy cannot provision a certificate for a host the domain does not reach.

## Modes

Production `live_readonly`:

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com --mode live-readonly --install-docker
```

Production paper:

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com --mode paper --install-docker
```

Local paper:

```bash
mkdir -p .docker/runtime
docker compose -f docker-compose.yml -f docker-compose.paper.yml -f docker-compose.local.yml up -d --build
```

Local research control plane:

```bash
mkdir -p .docker/research/runtime .docker/research/work
docker compose -f docker-compose.research.yml up -d --build
```

This starts only the research dashboard backend and the local research worker.
It does not start the trading bot, sidecar, agent, Caddy, or any remote host
process.

The research worker can claim artifact transfer records. Same-machine transfers use resumable append copies. Remote transfers use `rsync` over SSH with partial append verification, compression, and protected remote path arguments. Set `BUBA_RESEARCH_SSH_DIR` when the worker container needs a specific SSH directory; it is mounted at `/home/buba/.ssh`. Running transfers older than `BUBA_RESEARCH_TRANSFER_STALE_MS` are moved back to `retryable` so a restarted worker can resume from the partial destination.

Register remote source artifacts with `POST /api/research/artifacts/register` after finalization writes a manifest on the live machine. Use `POST /api/research/artifacts/import` only for artifact directories already present under the research work root.

Inventory dry-run plans:

```bash
python3 scripts/deploy-machine.py --machine live --dry-run
python3 scripts/deploy-machine.py --machine research --dry-run
```

Deploy or refresh the remote research stack on `testing`:

```bash
gh auth refresh -s write:packages
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research
```

The research deployment targets Ubuntu WSL on `testing`, uses digest-pinned
private GHCR images from `ops/research-images.lock.json`, syncs only the
research Compose file into `/home/testing/buba-paint-research`, preserves remote
`.env` and `.docker/research`, and starts only `research-dashboard` and
`research-worker`. The deploy runner sends the current `gh` token over SSH only
for a temporary GHCR auth config and removes that config before the session
exits. The generated `.env` includes `BUBA_RESEARCH_WORKER_TOKEN`,
`BUBA_RESEARCH_SSH_DIR=/home/testing/.ssh`,
`BUBA_RESEARCH_TRANSFER_STALE_MS=1800000`, and
`BUBA_DASHBOARD_CONFIG_DIR=./.docker/research/config`; the worker uses the token
when `BUBA_RESEARCH_CONTROLLER_URL` is set, uses the SSH directory for remote
artifact transfers, and recovers stale running transfers after the configured
window.

In the current public operator setup, `https://buba.toksaitov.com` is the only
dashboard URL. Caddy on `buba-paint` serves the live dashboard UI and proxies
`/api/research*` to the `testing` research stack through the managed
`buba-research-tunnel.service` and `buba-research-proxy.service` bridge. The
`testing` dashboard remains private infrastructure behind that route; operators
should not need to open a second dashboard.

Check the private research stack with:

```bash
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
ssh testing "curl.exe -s http://localhost:3002/health"
```

Stopped-live observability refresh on `buba-paint`:

```bash
gh auth refresh -s write:packages
python3 scripts/publish-live-images.py
python3 scripts/deploy-stopped-live.py --dry-run --expected-runtime-name <runtime-name> --expected-db-sha256 <finalized-db-sha256>
python3 scripts/deploy-stopped-live.py --expected-runtime-name <runtime-name> --expected-db-sha256 <finalized-db-sha256>
```

The stopped-live deploy uses `ops/live-images.lock.json`, pulls dashboard, agent, paint, and sidecar images by digest, and starts only `agent`, `dashboard`, and `caddy` against the existing finalized runtime from `~/buba-paint-live/current/.env`. It refuses non-dry-run deployment unless the current runtime name matches the expected finalized run, the live DB checksum matches the supplied SHA-256, and no `paint` or `sidecar` container is running. The paint and sidecar images are pulled for provenance and rollback readiness but are not started.

Research maintenance operations:

```bash
python3 scripts/research-maintenance.py status --machine research
python3 scripts/research-maintenance.py backup-db --machine research
python3 scripts/research-maintenance.py restore-db --machine research --backup <backup-id> --confirm
python3 scripts/research-maintenance.py collect-diagnostics --machine research
python3 scripts/research-maintenance.py rollback --machine research --to-ref HEAD~1 --confirm
python3 scripts/research-maintenance.py live-safety --machine live
```

Backups stay on `testing` under
`/home/testing/buba-paint-research/.docker/research/runtime/backups`.
Diagnostics bundles are redacted tarballs under `/tmp` on `testing`. Rollback
deploys an explicit previous image lock and then rolls forward to the current
tracked lock during rehearsal, leaving the final `testing` state on the latest
research images.

## Operations

Inspect services:

```bash
ssh buba-paint 'cd ~/buba-paint-live/current && sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml ps'
```

Tail logs:

```bash
ssh buba-paint 'tail -f ~/buba-paint-live/runtime/*/paint.log'
ssh buba-paint 'tail -f ~/buba-paint-live/runtime/*/sidecar.log'
```

Stop the stack:

```bash
ssh buba-paint 'cd ~/buba-paint-live/current && sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml down'
```

The dashboard password is generated per deployment and stored only on the host at the path printed by `scripts/deploy-docker.py`.

## Acceptance

* `http://buba.toksaitov.com` redirects to HTTPS.
* `https://buba.toksaitov.com/health` returns `{"ok":true}` with a valid certificate.
* `docker compose ps` shows Caddy, sidecar, bot, agent, and dashboard healthy or running.
* Runtime `paint.db` passes SQLite `PRAGMA quick_check`.
* `live_readonly` logs show no live order, cancel, redemption, or arming actions.
