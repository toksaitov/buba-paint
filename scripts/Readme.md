# Scripts

`scripts/` contains repo maintenance, setup, and manual analysis utilities.

Root-level scripts are active automation or setup entry points. Manual charting helpers live under `scripts/analysis/`. Historical one-off scripts live under `scripts/archive/`.

## Active Automation

* `audit-docs.py`: checks repo-owned `Readme.md` casing, local Markdown links, `data/` directory notes, stale file references, active-plan files under `docs/`, root scratch files, and transient derived SQLite WAL/SHM files. Run through `make docs-audit`.
* `audit-hot-path.py`: rejects known O(N) database scans, feed-loop SQLite calls, runtime replay validators, sidecar/account/control awaits, direct venue submissions from feed branches, opt-out `tick_data` defaults, and `quick_check` healthchecks from latency-sensitive bot and Docker runtime paths. Run through `make hot-path-audit`; it is also part of `make lint`.
* `check_coverage.py`: enforces component coverage floors for `make coverage-gate`.
* `deploy-docker.py`: stages and runs the Docker Compose/Caddy deployment on `buba-paint`. Run through `make docker-deploy`; use `make docker-deploy-dry-run` to inspect the selected host, domain, mode, and compose files without SSH mutations. The script verifies that the deployment domain points at the SSH host and creates swap on small hosts before building images.
* `live-readiness-host-soak.py`: stages a reviewed release on `buba-paint`, runs the no-order `live_readonly` host soak, and writes non-secret evidence under `data/experiments/replay-grade-readonly-soak-001/`. Run through `make live-readiness-host-soak`; use `LIVE_HOST_SOAK_ARGS="--dry-run"` to inspect the command plan without touching the host.
* `live-readiness-local.py`: runs the local live-money readiness gate and writes an evidence bundle outside the repository. Run through `make live-readiness-local`; use `LIVE_READINESS_ARGS="--dry-run"` for a safe manifest-only check.
* `live-low-latency-local.py`: runs hot-path audit, targeted writer/feed tests, and Compose config checks, then writes an evidence bundle under `/tmp` by default. Run through `make live-low-latency-local`; use `LIVE_LOW_LATENCY_ARGS="--dry-run"` to inspect commands only.
* `live-docker-smoke.py`: runs the local no-Caddy Docker `live_readonly` stack for 10 minutes on a Docker-native runtime volume, copies DB/log evidence to `/tmp`, and checks logs, restart counts, SQLite integrity, replay validation, feed classes, and zero live venue rows. Run through `make live-docker-smoke`; use `LIVE_DOCKER_SMOKE_ARGS="--dry-run"` to inspect the manifest only.
* `profile-live-runtime.py`: runs a standalone latency-only paper runtime profile against a `/tmp` DB, samples process CPU, and captures Linux `perf` output when available. Run through `make live-runtime-profile`; use `LIVE_RUNTIME_PROFILE_ARGS="--dry-run"` to inspect the evidence path only.
* `publish-live-images.py`: builds and pushes the registry-pinned live observability, bot, and sidecar images, then writes `ops/live-images.lock.json` with digest refs.
* `publish-research-images.py`: builds and pushes the registry-pinned research dashboard and worker images, then writes `ops/research-images.lock.json` with digest refs.
* `deploy-stopped-live.py`: refreshes `buba-paint` dashboard, agent, and Caddy from digest-pinned images while requiring the finalized live DB checksum and keeping `paint` and `sidecar` stopped.
* `deploy-machine.py`: deploys inventory-defined Compose stacks. Non-dry-run deploys are currently restricted to the `research` machine.
* `research-maintenance.py`: JSON-output operator utility for research status, DB backup, DB restore, diagnostics bundle collection, digest-lock rollback, and live safety snapshots.
* `ts_comment_audit.mjs`: enforces the TypeScript comment policy used by `make lint` and `make comment-audit`.

## Setup

* `setup-ubuntu.sh`: installs host dependencies and builds the Rust workspace, dashboard client, and Polymarket sidecar on an Ubuntu host.

## Manual Analysis

Use `scripts/analysis/` for quick legacy-compatible charts against an explicit run DB path. These helpers are not replay-grade sweep tooling and do not validate data quality.

## Archive

Use `scripts/archive/` for run-specific or old one-off scripts kept only for provenance. Archived scripts should not be treated as current workflow templates.

Do not write temporary DBs, logs, or scratch outputs into this directory. Use `/tmp` for throwaway work or `data/experiments/run-XXX-topic-NNN` for durable derived analysis.
