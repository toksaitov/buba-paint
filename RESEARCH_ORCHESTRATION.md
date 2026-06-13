# Research Orchestration Tracker

This tracker describes the current Research deployment and readiness state. It is
intentionally concise and holds only stable identity and current truth. Runtime
state (container status, telemetry samples, artifact sizes, checksums, and image
digests) lives in its executable sources, linked below. The dated evaluation
chronology and browser QA evidence live in
[docs/runs.md](docs/runs.md) and under `data/experiments/research-manual-qa-playwright/`.

## Current Status

The Phase 15 remote worker-controller protocol is deployed and accepted end to
end from the public dashboard `https://buba.toksaitov.com`. Jobs created on the
public dashboard are claimed by the worker on `testing` over the worker-token
protocol, run with live step and event updates, and produce reports whose JSON
and CSV documents are served by the public controller. The control-plane design
is documented in [docs/research-orchestration.md](docs/research-orchestration.md).

The operator-facing architecture is:

* `https://buba.toksaitov.com` is the only dashboard URL and the only operator
  dashboard for research. Monitor pages observe the stopped live-readonly run on
  `buba-paint`; Research pages are served from the same public dashboard, which
  is also the controller the worker talks to. Operators manage and view all
  research there.
* `testing` runs the `research-worker` container only, plus its storage, as
  private infrastructure. There is no local research dashboard on `testing`.
* Caddy on `buba-paint` proxies all public dashboard routes to the live dashboard
  container. The research worker on `testing` points
  `BUBA_RESEARCH_CONTROLLER_URL` at `https://buba.toksaitov.com` and
  authenticates to that same controller with `BUBA_RESEARCH_WORKER_TOKEN`,
  leasing and reporting all job, step, transfer, report, artifact, and telemetry
  work centrally.
* `paint` and `sidecar` stay stopped until the user explicitly starts a new live
  or paper run.

Known limitations:

* Job scratch archival deletes files under the work root of the machine that
  serves the dashboard API. Scratch DBs produced by a remote worker live on
  `testing` and are not deleted through the public controller yet.
* Export jobs assume the source runtime DB path is readable from the worker
  machine. Cross-machine export remains a manual finalize flow.
* The New Job form does not yet validate that a custom interval lies inside the
  artifact range, so a bad interval blocks at `prepare_backtest_input` rather
  than at form entry.

The current good live run is preserved locally and remotely. Its live DB
checksum is a fixed historical value:

`90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e`

The local safety backup is at
`data/live-run-backups/live-readonly-20260601-022105/live-readonly-20260601-022105-raw-20260601-183431Z`.

## Finish Line

Research is ready for operator evaluation when this workflow works from
`https://buba.toksaitov.com` without opening a second dashboard:

1. Confirm the stopped live run from Monitor pages.
2. Open Research > Artifacts and see the current stopped run plus prior
   finalized artifacts.
3. Create bounded current-params and sweep jobs from those artifacts.
4. Observe job, transfer, worker, and host state from the dashboard.
5. Recover or clone failed jobs with explicit operator confirmation.
6. Read report JSON and CSV.
7. Compare completed reports and understand ties, no-trade outcomes, and
   compatibility warnings.
8. Archive bulky scratch DBs without deleting durable reports or artifact data.
9. Back up, diagnose, and redeploy the research stack safely.

Later work may add richer scheduling, routing, or charting, but those are not
required for the next manual research evaluation unless a new critical gap is
found.

## Machines

Machine identity is stable; live status comes from `docker compose ps` and
`GET /api/research/machines/:id/telemetry`, not from this file.

### `buba-paint`

Public dashboard, Caddy edge, live monitor, and Research API proxy. Compose
services are `caddy`, `dashboard`, and `agent` (running) plus `paint` and
`sidecar` (stopped). Caddy proxies public dashboard UI and API traffic to the
live dashboard container, and the research worker authenticates to it with
`BUBA_RESEARCH_WORKER_TOKEN`. Health is at `https://buba.toksaitov.com/health`.

### `testing`

Research compute and artifact storage host. It runs the worker only and has no
local research dashboard. The worker leases work from and reports all results to
the central controller at `https://buba.toksaitov.com`
(`BUBA_RESEARCH_CONTROLLER_URL=https://buba.toksaitov.com`). It keeps a local
SQLite DB for its own telemetry, read directly by the maintenance tooling.

* SSH alias: `testing`
* WSL distro: `Ubuntu-24.04`
* Remote root: `/home/testing/buba-paint-research`
* Compose file: `docker-compose.research.yml`
* Services: `research-worker`

## Artifacts

Research > Artifacts on `https://buba.toksaitov.com` is the live source for
artifact identity, status, size, checksums, and intervals. Artifact registration
rejects WAL and SHM sidecars, so research artifacts must be stable single-file
SQLite backups whose manifest size matches the copied DB.

## Image Locks

Deployed image digests are pinned in the lock files, which are the single source
of truth:

* Research stack: `ops/research-images.lock.json` (dashboard and research worker).
* Stopped-live stack: `ops/live-images.lock.json` (dashboard, agent, paint, and
  sidecar).

Publish fresh images and update the relevant lock before the next real deploy if
code or image inputs change.

## Deployment Commands

Research publish and deploy:

```bash
python3 scripts/publish-research-images.py --dry-run
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine research
```

Stopped-live publish and deploy for the current stopped run:

```bash
python3 scripts/publish-live-images.py --dry-run
python3 scripts/publish-live-images.py
python3 scripts/deploy-stopped-live.py --dry-run --expected-runtime-name live-readonly-20260601-022105 --expected-db-sha256 90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e
python3 scripts/deploy-stopped-live.py --expected-runtime-name live-readonly-20260601-022105 --expected-db-sha256 90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e
```

Research maintenance:

```bash
python3 scripts/research-maintenance.py status --machine research
python3 scripts/research-maintenance.py backup-db --machine research
python3 scripts/research-maintenance.py restore-db --machine research --backup <backup-id> --confirm
python3 scripts/research-maintenance.py collect-diagnostics --machine research
python3 scripts/research-maintenance.py rollback --machine research --to-ref HEAD~1 --confirm
python3 scripts/research-maintenance.py live-safety --machine live
```

## Operating Rules

* Keep `buba-paint` safe. Do not start `paint` or `sidecar` unless the user
  explicitly approves a live or paper bot run.
* Use `buba.toksaitov.com` as the operator dashboard.
* Treat `testing` as research compute and storage infrastructure.
* Use Docker Compose plus scripts, not ad hoc remote commands, for deploys.
* Use digest-pinned GHCR images for deploys.
* Preserve live run backups and QA evidence on disk.
* Do not commit generated local backup directories.

## Fixture Seed Coverage

`scripts/seed-research-fixtures.py` seeds the operator-facing research tables:
users, sessions, research_machines, run_artifacts, artifact_transfers,
research_jobs, research_job_steps, research_job_events, and research_reports.
Its fixture reports are canonical `schema_version` 2 documents that match the
Rust report shape in `dashboard/server/src/research_reports.rs`, with one
explicit legacy `schema_version` 1 report retained for parse-path coverage.

The seed schema intentionally omits three tables that
`dashboard/server/src/db.rs` defines: `research_machine_telemetry_state`,
`research_machine_telemetry_samples`, and `research_job_templates`. The
dashboard exercises telemetry and templates through mocked query hooks in unit
tests, not through the seeded SQLite database, and there is no template fixture
consumer, so seeding those tables would add schema surface with no consumer.
Telemetry fixtures live in TypeScript as `fixtureMachineTelemetryState` and
`fixtureMachineTelemetryResponse` in
`dashboard/client/src/lib/research-fixtures.ts`. If a future workflow needs the
seeded DB to drive telemetry or templates end to end, extend the seed `SCHEMA`
and add populate helpers that mirror those `db.rs` definitions.
