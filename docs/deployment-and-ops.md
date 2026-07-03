# Deployment And Operations

This chapter describes how Buba is run locally and remotely. The preferred remote process model is Docker Compose with Caddy TLS. Legacy systemd/manual flows are reference-only unless a fresh plan explicitly chooses them.

## Operating Posture

The normal remote mode is `live_readonly`: authenticated account and venue monitoring, replay-grade public feed capture, shadow paper trading, agent, and dashboard. It does not arm real money, place orders, cancel orders, or redeem positions.

Real-money trading requires an explicit operator GO. Do not infer permission to trade from credentials, sidecar write endpoints, or dashboard controls.

Current host state (2026-07-03): the `buba-paint` host is temporarily staged off `live_readonly` in the `live_trading` plus `LIVE_DRY_RUN=true` disarmed canary stack for the live-readiness effort. It is parked: the `paint` bot container sleeps instead of running the live bot (via the `docker-compose.parked.yml` overlay, deployed with `deploy-docker.py --parked`), so it captures nothing and the runtime DB cannot grow, while `sidecar`, `agent`, `dashboard`, and `caddy` run normally. No order is possible while parked and disarmed. Reverting to `live_readonly` is a redeploy with `--mode live-readonly`. See the Parked Battle-Mode Staging section below and the [LIVE_READINESS_PLAN.md](../LIVE_READINESS_PLAN.md) handoff block.

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

## Live-Trading Canary Mode

`--mode live-trading` is the supervised single-order canary path. It uses
`docker-compose.live-trading.yml` and stays money-safe by default:

* `LIVE_DRY_RUN` defaults to `true`, so the bot builds order intents but never calls
  the venue.
* the bot always boots disarmed, so no order can be placed without an explicit arm
  control command even with dry-run off.
* the order sizes to about 5 USD using the production position fractions, and
  exposure is bounded by `LIVE_MAX_SINGLE_ORDER_USD=7` and
  `LIVE_MAX_OPEN_NOTIONAL_USD=7`, with `LIVE_MAX_SESSION_ORDERS=1` and
  `LIVE_MAX_SESSION_FILLS=1` bounding the run to one venue submission and one fill. The
  cash cap stays at the
  production 100 USD because the fractions need that bankroll to size a 5 USD order;
  see [../docs/canary-config.md](../docs/canary-config.md).

The Ireland host cannot build images, so live-trading deploys pull digest-pinned
images with `--use-locked-images`. Publish first so the lock matches the committed
source (`python3 scripts/publish-live-images.py`).

Rehearsal deploy (dry-run, no venue contact, safe to run any time):

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com \
  --mode live-trading --use-locked-images \
  --env-set LATENCY_ARB_MOMENTUM_THRESHOLD=<relaxed-from-data> \
  --env-set LATENCY_ARB_COOLDOWN_MS=15000
```

Real canary deploy (only on the operator GO; places at most one real order):

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com \
  --mode live-trading --use-locked-images \
  --env-set LIVE_DRY_RUN=false \
  --env-set LATENCY_ARB_MOMENTUM_THRESHOLD=<chosen-from-data> \
  --env-set LIVE_EXPECTED_EGRESS_IP=<server-egress-ip>
```

`--env-set KEY=VALUE` injects compose interpolation values into the generated
`.env`; it rejects deploy-reserved keys (`BUBA_*`, `COMPOSE_*`, secrets) and values
with newlines. `LIVE_DRY_RUN` is seeded to true in `.env` for live-trading unless
overridden. The exact canary overlay, the data-driven threshold method, and the
revert steps live in [../docs/canary-config.md](../docs/canary-config.md) and
[../CANARY_RUNBOOK.md](../CANARY_RUNBOOK.md).

## Parked Battle-Mode Staging

A staged `live_trading` stack captures replay-grade feed data. `live_trading`
requires `FEED_EVENT_STORAGE_PROFILE=replay_grade` (bootstrap refuses `compact`),
so a running dry-run stack grows the runtime DB by roughly 3.4 GB per day with no
auto-prune. The `LIVE_RUNTIME_MAX_DB_BYTES` guard only blocks new trading when
exceeded; it does not prune or stop capture. On the small Ireland host (29 GB disk)
an unattended dry-run stack fills the disk in about two days and wedges the bot even
though it never trades. The dashboard healthcheck only tests that `paint.db` and
`paint.log` exist, so a disk-full wedged bot still shows healthy in `docker ps`.

To hold the full stack ready for a canary GO without that growth, deploy parked:

```bash
python3 scripts/deploy-docker.py --host buba-paint --domain buba.toksaitov.com \
  --mode live-trading --use-locked-images --parked
```

`--parked` adds the `docker-compose.parked.yml` overlay, which overrides the `paint`
command to initialize an empty DB and then `sleep infinity` instead of running
`buba-paint live`. The bot captures nothing and the DB cannot grow, while `sidecar`,
`agent`, `dashboard`, and `caddy` run normally. The sidecar authenticates on demand
but reads "not ready" while parked because the parked bot does not poll it; that is
expected. To go live at GO, redeploy the same command without `--parked` so the bot
runs again.

Disk and teardown discipline for the canary:

* Do not leave a non-parked `live_trading` dry-run stack staged for more than about a
  day and a half on this host. For any unattended wait, park it or bring the live
  stack up fresh right before the GO.
* Each deploy creates a new `runtime/docker-<mode>-<stamp>/` directory and does not
  remove old ones, so prior-deploy DBs accumulate. Superseded
  `runtime/docker-live-trading-*` dry-run DBs are disposable (zero-trade; the canary
  needs a fresh DB anyway) and safe to delete; then `sudo docker system prune -af`.
* Preserve the `runtime/live-readonly-*` DBs (research capture) unless the operator
  explicitly approves deleting the exact files.

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

This starts only `research-worker`. It is for local orchestration testing and does not start the trading bot, sidecar, agent, Caddy, or any remote host process. The worker writes artifacts, prepared DBs, reports, and scratch outputs under `.docker/research/work` and keeps a local SQLite DB at `.docker/research/runtime/dashboard.db` for its own telemetry. Point `BUBA_RESEARCH_CONTROLLER_URL` at a controller dashboard to exercise the remote backend, or leave it unset to use the local SQLite database for offline testing.

Artifact transfers are worker-owned. Same-machine transfers use append-style resumable file copies. Remote transfers use `rsync` over SSH with partial append verification, compression, and protected remote path arguments. Set `BUBA_RESEARCH_SSH_DIR` when the worker container needs a specific SSH config or key directory; the Compose file mounts it at `/home/buba/.ssh`. Running transfer rows older than `BUBA_RESEARCH_TRANSFER_STALE_MS` are recovered to `retryable` on the next worker tick so a restarted worker can resume from partial files. Set it to `0` to disable automatic stale recovery. Stale recovery has no per-transfer lease owner, so it is only safe under exactly one transfer worker per research machine: a `running` row is then always the live worker's in-flight copy, and recovery must not fire while that copy is genuinely live. To enforce this, run a single `buba-research-worker` per `machine_id`, and a configured nonzero stale age is raised to a safety floor of one hour (`MIN_SAFE_STALE_AFTER_MS` in `research_transfer.rs`) so it clearly exceeds the worst-case single-file transfer time. The CLI default of `1800000` milliseconds is below that floor and is therefore raised to one hour at startup.

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

The publisher builds the dashboard and `research-worker` images, pushes private GHCR images, resolves their digests, and writes `ops/research-images.lock.json`. The deploy runner syncs only the research Compose file to `/home/testing/buba-paint-research`, preserves remote `.env` and `.docker/research`, authenticates to GHCR with a temporary Docker config using the current `gh` token, pulls the digest-pinned image, removes the temporary auth config, and starts `research-worker` only, without building on `testing`. The host runs no local research dashboard; the worker leases work from and reports to the central controller. The generated research worker token remains on `testing` under `/home/testing/buba-paint-research/.env`.

The GHCR token is not persisted on `testing`. If publishing or deployment reports missing package scopes, refresh local GitHub auth with:

```bash
gh auth refresh -s write:packages
```

The generated research `.env` sets `BUBA_RESEARCH_SSH_DIR=/home/testing/.ssh` and `BUBA_RESEARCH_TRANSFER_STALE_MS=1800000`. That lets the worker container use the WSL SSH alias `buba-paint` for live-to-research artifact transfers and recover killed transfer workers after the stale window. The worker image includes `rsync` and `openssh-client`.

Controller-based central control is the current model. `BUBA_RESEARCH_CONTROLLER_URL` together with `BUBA_RESEARCH_WORKER_TOKEN` makes the worker lease and persist all job, step, transfer, report, and artifact work through that controller over the worker-token API and report telemetry to it; the token authenticates every worker-token endpoint, not only heartbeats. The deployed research host runs `research-worker` only and points `BUBA_RESEARCH_CONTROLLER_URL` at the central dashboard `https://buba.toksaitov.com`, configured with the same `BUBA_RESEARCH_WORKER_TOKEN`. All research is managed and viewed on that single central dashboard; there is no co-located research dashboard on the worker host. The worker keeps a local SQLite DB only for its own telemetry, which the maintenance tooling reads directly. Without a controller URL the worker falls back to that local database, which is used for local orchestration testing rather than the deployed model.

Remote research checks. The host runs `research-worker` only and has no local health endpoint, so check that the worker container is up and tail its log:

```bash
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && tail -n 80 .docker/research/runtime/research-worker.log'"
```

The `ps` output should show `research-worker` running. Operator-facing research health lives on the central dashboard `https://buba.toksaitov.com/health`.

Research maintenance commands:

```bash
python3 scripts/research-maintenance.py status --machine research
python3 scripts/research-maintenance.py backup-db --machine research
python3 scripts/research-maintenance.py restore-db --machine research --backup <backup-id> --confirm
python3 scripts/research-maintenance.py collect-diagnostics --machine research
python3 scripts/research-maintenance.py rollback --machine research --to-ref HEAD~1 --confirm
python3 scripts/research-maintenance.py live-safety --machine live
```

`backup-db` uses SQLite backup semantics against
`/home/testing/buba-paint-research/.docker/research/runtime/dashboard.db` and
writes a manifest with size, SHA-256, `PRAGMA quick_check`, image refs, Compose
status, and research row counts. `restore-db` stops only the `testing`
research worker, writes a pre-restore safety backup, replaces the DB, removes
stale WAL/SHM sidecars, restarts the worker with the image refs recorded in the
backup manifest, and verifies the worker through Compose status and worker
telemetry.
`collect-diagnostics` writes a redacted tarball under `/tmp` on `testing`;
it excludes DB files, artifacts, reports, SSH keys, and Docker auth configs.
`rollback` deploys a previous digest lock, verifies health, and rolls forward
to the current tracked lock so the final `testing` state stays current.

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
