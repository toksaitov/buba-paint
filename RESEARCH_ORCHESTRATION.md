# Research Orchestration Tracker

This is the active tracker for dashboard-centered research orchestration. It is
the current source for what exists, what was verified, which machines are
involved, and what still gates readiness.

## Current Status

Status: Phase 12 readiness is complete. Phase 13 local cleanup is complete.

Phases 1 through 12 are implemented. Phase 12 was the stopped-live deployment
and exhaustive Research QA pass. It started at `2026-05-31T10:55:17Z`, crossed
the required 3-hour floor, continued through the user-requested extended QA
hour, and completed after final evidence at `2026-05-31T14:55:10Z`.

Phase 13 was local-only cleanup. It did not deploy, publish, delete evidence,
or change the readiness finish line. It added ignore coverage for generated
manual QA evidence, tightened Research list and dialog behavior tests, cleaned
comment-policy gaps, and kept publish/deploy defaults strict.

Completion evidence:

* latest research images are deployed on `testing`.
* latest stopped-live observability images are deployed on `buba-paint`.
* `paint` and `sidecar` stayed stopped on `buba-paint`.
* browser-controlled Research QA found no remaining critical or major defects.
* local gates and remote safety evidence are recorded.

Current local caveat:

* Phase 13 touched image inputs after the last Phase 12 publish. The image lock
  files below are still the deployed Phase 12 locks, but deploy dry-runs now
  correctly report them as stale until the next publish from committed source.

The current implementation can:

* Deploy the research stack to `testing` with digest-pinned GHCR images.
* Deploy stopped-live observability on `buba-paint` without starting bot
  services.
* Observe the research host from Research > Machines.
* Register, import, verify, archive, restore, and delete eligible artifacts.
* Create, inspect, pause, resume, retry, cancel, verify, and delete transfer
  records.
* Create current-params and sweep jobs from forms or shared templates.
* Run real backtests and sweeps on the research host.
* Cancel active research child commands.
* Retry, continue, clear stale leases, resolve blockers, and clone jobs with
  edited params.
* Archive scratch DBs while preserving report JSON, report CSV, artifact files,
  manifests, and checksums.
* Generate schema v2 current-params and sweep reports.
* Rank, inspect, filter, and compare reports from the dashboard.
* Inspect queue, attention, retention, templates, machine telemetry, and
  recovery state from the dashboard.
* Back up and restore the research dashboard DB on `testing`.
* Collect redacted diagnostics bundles on `testing`.
* Roll back and roll forward digest-pinned research deployments.

## Finish Line

Phase 12 is the last planned readiness phase. The finish line is a
production-like rehearsal, not infinite feature work.

Research is ready for the next paper-run research/backtesting cycle when an
operator can complete this workflow without shell workarounds:

1. Use the latest live observability dashboard on `buba-paint` while the bot
   remains stopped.
2. Confirm the finalized live-readonly DB and artifact are preserved.
3. Use `testing` as the research host.
4. Create bounded current-params and sweep jobs from the dashboard.
5. Observe job, transfer, worker, and host state from the dashboard.
6. Recover or clone failed work with explicit operator confirmation.
7. Read report JSON and CSV.
8. Compare completed reports and understand ties, no-trade outcomes, and
   compatibility warnings.
9. Archive bulky scratch DBs without deleting durable reports or artifact data.
10. Back up, diagnose, and redeploy the research stack safely.

Later work may add more orchestration, scheduling, or visualization, but it is
not required for the next paper-run research/backtesting cycle unless a new
critical gap is found.

## Machines

### `buba-paint`

Purpose: stopped live observability against the finalized live-readonly runtime.

Dashboard surface: Monitor pages, including Monitor > Machine.

Phase 12 rules:

* `dashboard`, `agent`, and `caddy` may run.
* `paint` and `sidecar` must stay stopped.
* The finalized DB must keep the expected checksum.

Finalized DB:

* path:
  `/home/ubuntu/buba-paint-live/runtime/live-readonly-20260514-184119/paint.db`
* size: `6342098944`
* SHA-256:
  `2f3a778d9955117f7468bec6e459742f7d17417ce8287c7681b61231fba75a81`

Latest Phase 12 stopped-live evidence:

* `buba-paint-dashboard-1`: `edfa53b48405`, running, healthy.
* `buba-paint-agent-1`: `bc71ba1a64e9`, running, healthy.
* `buba-paint-caddy-1`: `2deb6c87a582`, running.
* `buba-paint-paint-1`: `1e408676cf2a`, exited.
* `buba-paint-sidecar-1`: `b587808b24b9`, exited.
* public health endpoint returned `{"ok":true}`.

### `testing`

Purpose: research dashboard and research worker host.

Environment:

* SSH alias: `testing`
* WSL distro: `Ubuntu-24.04`
* Remote root: `/home/testing/buba-paint-research`
* Compose file: `docker-compose.research.yml`
* Browser tunnel: `127.0.0.1:3302 -> testing:localhost:3002`

Latest Phase 12 research evidence:

* `research-dashboard`: `ba99467851a3`, healthy.
* `research-worker`: `459dcfdfbfa5`, running.
* telemetry: `stale=false`, worker status `idle`, sample count `60`.
* DB counts after the post-fix smoke: 18 jobs, 13 reports, 3 artifacts,
  3 transfers, 1 template.

## Current Image Locks

These locks identify the latest deployed Phase 12 images. They are stale
relative to the uncommitted Phase 13 cleanup worktree and must be refreshed by
publishing images after the next commit before another real deploy.

### Research Images

`ops/research-images.lock.json`:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:da214be1035526e3c9714b0f02135222407216f0d7f896a254b8fdc750477b51`
* research worker:
  `ghcr.io/toksaitov/buba-paint-research-worker@sha256:562363b07bf42aa79d421763d164aa5c853e9255acdb4fd90f1985d6fba86a38`

### Stopped-Live Images

`ops/live-images.lock.json`:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:efc0b1a3294b94cc66d66924e29c29f7632e776eaec85cf3ce3c264ef6faf3e6`
* agent:
  `ghcr.io/toksaitov/buba-paint-agent@sha256:1b546bd65a1f24aff6eca2a2ab1ae5a1f9a18a2b94f4889525cc01b42f4321f1`
* paint image published but not running:
  `ghcr.io/toksaitov/buba-paint-bot@sha256:69d8232deeb026d91f89852ca080dff07e2e11bfe446155bed00995cfddf4ae3`
* sidecar image published but not running:
  `ghcr.io/toksaitov/buba-paint-sidecar@sha256:0afc95de0967ff26fddb1abf4469cfe4c0056b9ad1a4f242cb6afa4f130adeec`

## Deployment Commands

Research publish and deploy:

```bash
python3 scripts/publish-research-images.py --dry-run
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine research
```

Stopped-live publish and deploy:

```bash
python3 scripts/publish-live-images.py --dry-run
python3 scripts/publish-live-images.py
python3 scripts/deploy-stopped-live.py --dry-run --expected-db-sha256 2f3a778d9955117f7468bec6e459742f7d17417ce8287c7681b61231fba75a81
python3 scripts/deploy-stopped-live.py --expected-db-sha256 2f3a778d9955117f7468bec6e459742f7d17417ce8287c7681b61231fba75a81
```

Do not use the normal live deploy path to start `paint` or `sidecar` during
Phase 12.

## Phase 12 Browser QA Evidence

Evidence workspace:

* `data/experiments/research-manual-qa-20260531-105517`

Representative artifact:

* `live-readonly-20260514-184119-finalized-20260517-075706Z`

Known-good interval:

* local: `2026-05-17 13:39` to `2026-05-17 13:41`
* UTC: `2026-05-17T07:39:00.000Z` to `2026-05-17T07:41:00.000Z`
* persisted:
  * `start_ms=1779003540000`
  * `end_ms=1779003660000`

Phase 12 browser-created or reused jobs:

* current-params job: `e0d1c900-fe26-4a17-a427-0bedf1fca140`
* current-params report: `c9b54b3d-8b2a-432a-923e-fcca76879ca0`
* minimal sweep job: `f9816a48-7320-46ac-adcf-499b624e1e82`
* minimal sweep report: `0389a082-b309-45a4-98c7-a4a44618d449`
* post-fix current-params job:
  `93b44630-0e6d-4d7a-b4fd-68a7d124a104`
* post-fix current-params report:
  `cf04e6f8-15a7-4d3e-a75f-e80860a719bc`

Detailed click-by-click QA chronology, screenshots, command outputs, and issue
notes live in:

* `data/experiments/research-manual-qa-20260531-105517/notes.md`

Final QA coverage summary:

* Research home, Machines, Artifacts, Transfers, Jobs, Reports, Templates,
  Retention, and comparison routes were exercised through the browser.
* Current-params and minimal sweep workflows completed on `testing`.
* Report JSON, report CSV, schema v2 metrics, scratch archive, recovery
  diagnosis, and comparison workflows were verified.
* Browser back, forward, reload, detail return links, and direct query URLs were
  verified for list-state preservation.
* Defects found during QA were fixed and covered with focused tests where
  practical.

## Latest Verification Gates

Passed after Phase 13 local cleanup:

* `cd dashboard/client && npm run lint`
* `cd dashboard/client && npm test`
* `cd dashboard/client && npm run build`
* `node scripts/ts_comment_audit.mjs`
* stable-toolchain Rust comment policy check
* stable-toolchain `cargo fmt --all --check`
* stable-toolchain `cargo test -p buba-dashboard`
* stable-toolchain `cargo test --workspace`
* `cd polymarket-sidecar && npm test`
* `cd polymarket-sidecar && npm run build`
* `python3 scripts/tests/test_research_maintenance.py`
* `python3 scripts/audit-docs.py`
* `git diff --check`
* `docker compose -f docker-compose.research.yml config --quiet`
* `python3 scripts/publish-research-images.py --dry-run`
* `python3 scripts/publish-live-images.py --dry-run`

Expected local cleanup caveats:

* `python3 -m pytest scripts/tests` is unavailable in the local Python
  environment because `pytest` is not installed. The direct script test above
  passed.
* `python3 scripts/deploy-machine.py --machine research --dry-run` refuses the
  stale research image lock after Phase 13 cleanup edits.
* `python3 scripts/deploy-stopped-live.py --dry-run --expected-db-sha256
  2f3a778d9955117f7468bec6e459742f7d17417ce8287c7681b61231fba75a81` reports
  the stale live image lock after Phase 13 cleanup edits.

Latest Phase 12 deploy checks:

* live stopped deploy completed with `paint` and `sidecar` still exited.
* research deploy completed with current digest-pinned images.
* research DB backup created:
  `dashboard-db-20260531-131638`
* backup `PRAGMA quick_check`: `ok`
* backup SHA-256:
  `d8787b5c2965000ae16bb73de0c87de6e3bf3894ed20b45011793001cfe771e4`
* diagnostics bundle created:
  `buba-research-diagnostics-20260531-131638`

Final closeout evidence:

* user-requested extended QA hour completed at `2026-05-31T14:55:10Z`.
* final docs audit passed.
* final `git diff --check` passed.
* final `testing` status:
  `data/experiments/research-manual-qa-20260531-105517/research-status-final-1455.json`
* final `buba-paint` safety status:
  `data/experiments/research-manual-qa-20260531-105517/live-safety-final-1455.json`

## Operating Rules

* Keep `buba-paint` safe. Do not start `paint` or `sidecar` unless the user
  explicitly approves a live bot run.
* `testing` is the research host for backtesting and sweeps.
* Use Docker Compose plus scripts, not ad hoc remote commands, for deploys.
* Use digest-pinned GHCR images for research deploys.
* Keep stopped-live observability separate from real bot execution.
* Treat machine identity, telemetry, and deployment evidence as separate:
  * DB and inventory define identity.
  * Worker and agent telemetry define observed host state.
  * Compose proves container deployment and health.
* Research > Machines is research-host observability, not machine CRUD.
* Machine CRUD and lifecycle APIs remain script/API surfaces.
