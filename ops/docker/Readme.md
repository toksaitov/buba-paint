# Docker Deployment

Docker Compose with Caddy is the preferred single-host deployment model.

## Requirements

- DNS: `buba.toksaitov.com` points at the `buba-paint` host.
- Firewall: inbound TCP `80` and `443` are open for Caddy and Let's Encrypt.
- SSH: `ssh buba-paint` works from the operator machine.
- Local secrets: `.secrets/buba-paint-live-sidecar.env` exists and is not committed.
- Host capacity: small AWS hosts need swap for image builds. The deploy runner creates a 4 GiB swap file by default when no swap is active.

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

- `http://buba.toksaitov.com` redirects to HTTPS.
- `https://buba.toksaitov.com/health` returns `{"ok":true}` with a valid certificate.
- `docker compose ps` shows Caddy, sidecar, bot, agent, and dashboard healthy or running.
- Runtime `paint.db` passes SQLite `PRAGMA quick_check`.
- `live_readonly` logs show no live order, cancel, redemption, or arming actions.
