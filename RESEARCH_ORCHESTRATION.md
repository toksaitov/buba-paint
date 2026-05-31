# Research Orchestration Tracker

This is the active handoff and implementation tracker for dashboard-centered
research orchestration. It is no longer a chronological phase ledger. The old
phase labels were useful while building the first version, but they became
confusing after UI handoff, remote deployment, telemetry, recovery, and browser
smokes were completed in overlapping tracks.

Use this file to answer:

* What exists now.
* Which machines are involved.
* What was verified.
* What must not be disturbed.
* What the finite research-ready finish line is.

## Current Status

Status: phases 1-8 are implemented and smoke-tested on `testing`.

There is no active implementation phase at the moment. The completed historical
work remains phases 1-7T plus canonical Phase 8. The remaining roadmap is
finite: phases 9-11 are required before calling the Research section ready for
the next paper-run research/backtesting cycle. Anything outside those three
phases is explicitly later work.

The system can now:

* Deploy the research stack to `testing` with digest-pinned GHCR images.
* Observe the research host from Research > Machines.
* Register/import artifacts.
* Transfer artifacts.
* Create current-params and sweep jobs.
* Run real backtests on the research host.
* Cancel active child commands.
* Archive scratch DBs while preserving reports.
* Generate schema v2 report JSON and CSV for current-params and sweep jobs.
* Rank, inspect, and compare reports from the dashboard.
* Diagnose blocked/failed jobs.
* Clone blocked/failed jobs with edited params and guarded intervals.

## Research-Ready Finish Line

Research is ready for the next paper-run research/backtesting cycle when an
operator can complete this workflow without shell workarounds:

1. Finalize or select a live/paper run artifact.
2. Transfer or register that artifact on `testing`.
3. Create bounded current-params and sweep jobs with explicit intervals.
4. Observe job, transfer, worker, and host state from the dashboard.
5. Cancel, retry, continue, clear stale leases, resolve blockers, or clone with
   edited params when a run fails.
6. Inspect generated report JSON/CSV and dashboard summaries.
7. Compare completed runs well enough to decide which result is better.
8. Archive bulky scratch DBs while preserving durable reports and provenance.
9. Redeploy, roll back, collect logs, and restore the research dashboard DB if a
   deployment or host problem occurs.
10. Prove the whole workflow in a browser-controlled smoke on `testing` while
    confirming `buba-paint` live containers stay unchanged.

The finish line is not "all possible research tooling forever." It is the
minimum professional system needed to run the next paper-run research cycle
reliably, understand the result, recover from common failures, and avoid
disturbing the live bot.

## Operating Rules

* Keep the live `buba-paint` machine safe. Do not restart or redeploy the live
  bot unless the user explicitly asks for a live-machine phase.
* `testing` is the research host. It runs Ubuntu WSL and Docker.
* Use Docker Compose plus inventory-driven scripts for deployment.
* Use registry-pinned GHCR images for the `testing` research deploy.
* Keep the live deploy on the existing non-registry path until a separate
  live-specific rollout is planned.
* Treat machine identity, telemetry, and management as separate concerns:
  * DB and `ops/research-machines.toml` define identity.
  * Worker/agent telemetry defines observed host state.
  * Compose proves deployment and container health.
* Research > Machines is observability for research hosts, not machine CRUD.
* Machine CRUD/lifecycle APIs remain available for scripts and recovery.

## Machines

### `buba-paint`

Purpose: live bot host.

Dashboard surface: Monitor > Machine.

Safety baseline from the latest research deploy smoke:

* `buba-paint-agent-1`: `50003527ee0e`
* `buba-paint-caddy-1`: `2deb6c87a582`
* `buba-paint-dashboard-1`: `773286fe7eb7`

These IDs stayed unchanged during the latest research deploy and browser smoke.

### `testing`

Purpose: research dashboard and research worker host.

Environment:

* SSH alias: `testing`
* WSL distro: `Ubuntu-24.04`
* Remote root: `/home/testing/buba-paint-research`
* Compose file: `docker-compose.research.yml`
* Browser tunnel used for smoke: `127.0.0.1:3302 -> testing:localhost:3002`

Running services:

* `research-dashboard`
* `research-worker`

Research dashboard health:

```bash
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'curl -sf http://localhost:3002/health'"
```

## Current Research Images

Current deployed digest refs in `ops/research-images.lock.json`:

* Dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:e1b858fa24b57985516a23ea0f40eaa4c9613b19bf481e038921f44b49c6bfa7`
* Research worker:
  `ghcr.io/toksaitov/buba-paint-research-worker@sha256:ae7d295389afd633a01bd7607cdc176863b51be3f4b096025b9586e8116597ad`

Publish/deploy path:

```bash
python3 scripts/publish-research-images.py --dry-run
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine research
```

Do not run the non-dry-run deploy command for `live` unless a live-machine phase
has been explicitly approved.

## Implemented Capability Areas

### Control-Plane Data Model

Implemented:

* Research machines.
* Artifacts.
* Transfers.
* Jobs.
* Job steps.
* Job events.
* Reports.
* Typed research machine telemetry state.
* Typed research machine telemetry sample history.

Admin users can mutate research entities. Observers can read operational state
but cannot perform recovery, lifecycle, delete, or create operations.

### Artifacts And Transfers

Implemented:

* Artifact manifest and checksum handling.
* Safe artifact path handling under configured research roots.
* Artifact import and remote manifest registration.
* Artifact verify, metadata update, archive, restore, and guarded delete.
* Transfer create, pause, resume, retry, verify, cancel, and delete.
* Worker transfer execution with resumable local copies and remote `rsync`.
* Stale running transfer recovery.

Known finalized artifact used for smoke:

* `live-readonly-20260514-184119-finalized-20260517-075706Z`

Research-machine artifact root:

* `/home/testing/buba-paint-research/.docker/research/work/artifacts/live-readonly-20260514-184119-finalized-20260517-075706Z`

### Job Execution

Implemented:

* Local research worker and remote research worker boundary.
* Deterministic job step creation.
* Step leasing and recovery.
* Current-params backtest pipeline.
* Sweep pipeline.
* Command-backed execution through an allowlisted executor boundary.
* Active child command supervision.
* Cancellation of running child commands.
* Retry, continue, step retry, stale lease clear, and blocker resolution.

Backtest/sweep interval behavior:

* Explicit Start/End submit as `start_ms` and `end_ms`.
* Artifact fallback is available but visible.
* Invalid, missing, reversed, large, or fallback-derived intervals are guarded.
* Large interval threshold is 6 hours.

### Reports And Scratch Cleanup

Implemented:

* Report metadata rows.
* Report JSON and CSV file reads.
* Report rename, archive, restore, delete record, and delete with files.
* Report regeneration from durable job/step/event/report metadata.
* Schema v2 report generation for current-params jobs.
* Schema v2 report generation for sweep jobs.
* Current-params metrics:
  * net PnL
  * gross PnL
  * fees
  * final balance
  * trade count
  * wins and losses
  * win rate
  * max drawdown
  * signal count
  * fill/no-fill counts when present
  * equity curve
  * drawdown curve
  * rejection summaries
  * no-trade and no-signal diagnostics
* Sweep analysis:
  * parameter column detection
  * metric column preservation
  * ranking by `pnl`
  * malformed row diagnostics
* Report list filtering and sorting by job type, artifact, status, analysis
  availability, Net PnL, drawdown, win rate, trades, and updated time.
* Report detail metrics, provenance, equity/drawdown charts, diagnostics, raw
  JSON, CSV preview, and sweep ranked rows.
* Report comparison route:
  `/research/reports/compare?ids=<id>,<id>`
* Comparison warnings for different job types, artifacts, intervals, and
  balances.
* Manual job scratch archive:
  `POST /api/research/jobs/:id/archive-scratch`

Scratch archive deletes only prepared/backtest SQLite families under the job
root. It preserves report JSON, report CSV, report metadata, artifact files,
manifests, and checksums.

### Machine Observability

Implemented:

* Shared Rust host telemetry contract in `crates/buba-machine-telemetry`.
* Existing bot Machine page remains response-compatible.
* Research worker samples CPU, per-core CPU, load, memory, swap, work-root disk,
  host identity, sampler health, worker status, and activity.
* Research telemetry persistence:
  * latest state row per machine
  * sample history with bounded retention
* Authenticated telemetry endpoint:
  `GET /api/research/machines/:id/telemetry`
* Research > Machines is read-only and research-host-only.

Latest smoke telemetry for `research` showed:

* worker status: `idle`
* stale: `false`
* sample count: `60`
* disk mount: `/research`
* sampler error: `null`

### Recovery UX

Implemented:

* Job detail recovery diagnosis panel for blocked, failed, retryable, and stale
  active lease states.
* Diagnosis extracts:
  * step
  * attempts
  * started/completed timing
  * lease owner and lease expiration
  * command program
  * command args
  * command working directory
  * status code
  * stdout
  * stderr
  * raw event JSON
  * raw step JSON
* Guidance distinguishes:
  * stale running lease
  * transient retry candidate
  * deterministic blocker where retrying same inputs may repeat failure
  * `prepare_backtest_input` missing open/close boundary prices
* Job detail Clone now opens an edit/confirm dialog.
* Clone dialog pre-fills:
  * job type
  * artifact
  * priority
  * Start/End
  * balance
  * `--set`
  * sweep dimensions
  * additional unknown params JSON
* Observers can inspect diagnosis and clone params, but cannot submit mutations.

## Latest End-To-End Evidence

### Browser Recovery Smoke

Target: `testing` through `http://127.0.0.1:3302`.

Source blocked job:

* `7c8e48df-bb5e-4323-9f50-63d06a0db5e7`

Observed diagnosis:

* failed step: `prepare_backtest_input`
* status code: `1`
* stderr included:
  `missing_open_prices=1,missing_close_prices=1`

Browser-created clone:

* job: `dfd84955-5944-4db1-9f45-83e43eb8e7ad`
* report: `21022f09-95e1-4842-9598-4b41e29c1e3e`
* local interval: `2026-05-17 13:39` to `2026-05-17 13:41`
* persisted params:
  * `start_ms=1779003540000`
  * `end_ms=1779003660000`
* all six steps completed
* report JSON loaded in browser
* report CSV loaded in browser
* Research > Machines > research returned to idle with fresh telemetry
* `testing` worker process list showed only `buba-research-worker`
* `buba-paint` container IDs stayed unchanged

### Browser Results And Comparison Smoke

Target: `testing` through `http://127.0.0.1:3302`.

Deployed image refs:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:e1b858fa24b57985516a23ea0f40eaa4c9613b19bf481e038921f44b49c6bfa7`
* research worker:
  `ghcr.io/toksaitov/buba-paint-research-worker@sha256:ae7d295389afd633a01bd7607cdc176863b51be3f4b096025b9586e8116597ad`

Reports verified in the browser:

* current-params report:
  `21022f09-95e1-4842-9598-4b41e29c1e3e`
* current-params report:
  `565561ba-05d6-4dd0-9dce-963399084018`
* sweep report:
  `bbda6818-ca1d-4058-b12a-49892d115df1`

Observed:

* Reports list sorted schema v2 reports by Net PnL first.
* Legacy schema v1 reports rendered as analysis unavailable.
* Current-params detail showed Net PnL, final balance, trades, win rate,
  drawdown, signals, wins, losses, provenance, worker image, rejection reasons,
  no-trade/no-signal diagnostics, equity chart, drawdown chart, JSON, and CSV.
* Sweep detail showed two ranked `LATENCY_ARB_MIN_ASK` rows and preserved the
  sweep metric columns.
* Comparison loaded the two current-params reports and the sweep report.
* Comparison showed a job-type compatibility warning.
* Comparison showed an explicit no-winner state because top Net PnL was tied.
* Research > Machines > research returned to idle with fresh telemetry.
* `buba-paint` container IDs stayed unchanged before and after deploy and smoke.

### Prior Workflow Smoke

Successful bounded current-params job:

* job: `f8b4e4a5-5055-4345-9f2a-e89843ea4b2f`
* report: `4d330f43-2b3d-4be2-8c3c-ca49f8fed1b1`
* interval: `2026-05-17 13:39` to `2026-05-17 13:41` local
* persisted params:
  * `start_ms=1779003540000`
  * `end_ms=1779003660000`

Scratch archive smoke:

* `prepared-backtest.db` and `backtest.db` deleted
* `report.json` and `report.csv` preserved

Cancellation smoke:

* job: `1e0b42ea-fdb3-4776-8655-23ec10df4530`
* active child command was terminated
* job/steps stayed cancelled
* no lingering `buba-paint` child process remained

## Verification Commands

Normal local gates:

```bash
cd dashboard/client && npm run lint
cd dashboard/client && npm test
cd dashboard/client && npm run build
python3 scripts/audit-docs.py
docker compose -f docker-compose.research.yml config --quiet
python3 scripts/publish-research-images.py --dry-run
python3 scripts/deploy-machine.py --machine research --dry-run
git diff --check
```

Rust gate:

```bash
cargo test -p buba-dashboard
```

Local note: the operator machine has previously had a broken Cargo proxy
invocation. If plain `cargo` fails with unexpected proxy arguments, run the
stable toolchain binary directly:

```bash
RUSTC=/Users/toksaitov/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/toksaitov/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/toksaitov/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test -p buba-dashboard
```

Remote research deploy gate:

```bash
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
```

Live safety check:

```bash
ssh buba-paint "docker ps --format 'table {{.Names}}\t{{.ID}}\t{{.Status}}\t{{.Image}}' | sed -n '1,20p'"
```

## Important Files

Backend and worker:

* `dashboard/server/src/api/research.rs`
* `dashboard/server/src/db.rs`
* `dashboard/server/src/research_worker.rs`
* `dashboard/server/src/research_pipeline.rs`
* `dashboard/server/src/research_reports.rs`
* `dashboard/server/src/research_transfer.rs`
* `crates/buba-machine-telemetry`

Frontend:

* `dashboard/client/src/pages/research-job-detail.tsx`
* `dashboard/client/src/components/research/job-create-form.tsx`
* `dashboard/client/src/components/research/job-clone-dialog.tsx`
* `dashboard/client/src/components/research/job-recovery-diagnosis.tsx`
* `dashboard/client/src/pages/research-machine-detail.tsx`
* `dashboard/client/src/pages/research-reports.tsx`
* `dashboard/client/src/pages/research-report-detail.tsx`
* `dashboard/client/src/pages/research-report-compare.tsx`
* `dashboard/client/src/lib/research-report-analysis.ts`

Deployment and inventory:

* `docker-compose.research.yml`
* `ops/research-machines.toml`
* `ops/research-images.lock.json`
* `scripts/publish-research-images.py`
* `scripts/deploy-machine.py`

Fixtures and tests:

* `dashboard/client/src/lib/research-fixtures.ts`
* `dashboard/client/src/pages/__tests__/research-job-detail.test.tsx`
* `dashboard/client/src/pages/__tests__/research-reports.test.tsx`
* `dashboard/client/src/pages/__tests__/research-report-detail.test.tsx`
* `dashboard/client/src/pages/__tests__/research-report-compare.test.tsx`
* `dashboard/client/src/lib/__tests__/research-report-analysis.test.ts`
* `dashboard/client/src/components/research/__tests__/job-create-form.test.tsx`
* `dashboard/server/src/tests/api_research_tests.rs`
* `dashboard/server/src/tests/db_tests.rs`
* `dashboard/server/src/tests/research_worker_tests.rs`

## Remaining Phases

Do not create new lettered phases. Continue from the completed 1-7T history and
use phases 9-11 until the research-ready finish line is reached.

### Phase 8: Results And Comparison

Status: complete.

Goal: make completed runs useful to inspect and compare.

Required deliverables:

* Report detail shows the key metrics needed to judge a backtest result.
* Equity, drawdown, and profit/loss series are readable from the dashboard.
* Report list supports practical filtering, sorting, and status scanning.
* Completed current-params runs can be compared side by side.
* Sweep results can be browsed by parameter set and ranked by chosen metrics.
* Parameter, interval, artifact, code/image digest, and source-job provenance are
  visible from report and comparison views.
* Report JSON/CSV loading, missing data, malformed data, empty runs, and chart
  edge cases are covered by tests.

Stop condition:

* Complete. Browser smoke on `testing` opened two completed current-params
  reports and one sweep report, compared them, showed a compatibility warning,
  and showed the correct no-winner state for tied Net PnL.

### Phase 9: Operator Queue, Templates, And Retention

Goal: make day-to-day operation of research history and queue state complete.

Required deliverables:

* Jobs, transfers, artifacts, and reports have practical filters for active,
  failed, blocked, stale, completed, archived, and deleted states.
* Queue state is easy to inspect: running work, waiting work, retryable work,
  blocked work, and disabled-host impact.
* Job creation supports saved/reusable templates for common current-params and
  sweep shapes.
* Bulk cleanup/archive flows exist for completed old jobs and reports, with
  guarded confirmation and clear skipped/deleted summaries.
* Destructive actions remain admin-only and are tested for observers.
* Retention status makes bulky scratch DB usage visible before cleanup.

Stop condition:

* A browser smoke on `testing` can create from a template, recover or classify
  blocked work, archive completed scratch/report history safely, and leave the
  dashboard in a clear state.

### Phase 10: Deployment, Backup, Rollback, And Diagnostics

Goal: make research deployment and incident handling boring enough for repeated
use.

Required deliverables:

* Registry-pinned deploy has a tested rollback path to the previous image lock.
* Research dashboard DB backup and restore commands exist and are documented.
* Remote log bundle collection captures compose status, dashboard logs, worker
  logs, image refs, health, telemetry, and recent job/transfer failures.
* Deploy health gates fail before replacing containers when required inputs are
  missing or registry auth fails.
* Failed deploys and failed health checks produce actionable diagnostics.
* `buba-paint` safety checks are part of every research deploy smoke.

Stop condition:

* A controlled deploy, rollback, log-bundle, backup, and restore rehearsal passes
  on `testing` without touching live containers.

### Phase 11: Paper-Run Readiness Rehearsal

Goal: prove the whole Research section is ready for the next real paper-run
research/backtesting cycle.

Required deliverables:

* Use a finalized paper/live artifact representative of the next real run.
* Run one bounded current-params backtest.
* Run one bounded sweep or the smallest representative sweep agreed for the
  cycle.
* Verify telemetry, queue state, cancellation/recovery controls, report loading,
  comparison, scratch archive, and retention state.
* Verify deployment rollback and DB backup artifacts exist before the rehearsal.
* Record the exact artifact, job IDs, report IDs, image digests, machine state,
  and live-container safety evidence in this file.

Stop condition:

* The user can start the next paper-run research/backtesting cycle from the
  dashboard with no known critical workflow gaps.

## Later Work, Not Required For The Finish Line

These are useful, but they must not expand the current finish line unless the
user explicitly changes priorities:

* Multiple research hosts and capacity-based routing.
* Prometheus/Grafana or a separate telemetry service.
* Live-machine registry-pinned deploy.
* Automatic retry or automatic interval edits.
* Advanced portfolio analytics beyond the first comparison/reporting pass.

## Retired Historical Phase Map

This section exists only to explain old references in commits or chat history.
Do not use these labels for new planning.

* Phases 1-6: local contracts, schema, worker, artifact format, command-backed
  pipeline, export integration, and local Compose/deploy prep. Complete.
* Phase 7 and lettered 7B-7T work: remote integration, operator APIs, transfer
  executor, scratch archive, lifecycle APIs, report regeneration, host
  observability, lint cleanup, registry-pinned deploy, workflow gap fixes, and
  blocked-run recovery UX. Complete.
* Temporary UI handoff track: this was previously called `Phase 8` in the old
  tracker, but it was parallel UI handoff work, not the canonical next backend
  roadmap phase. It is retired after merge.
* Temporary fixture-seeding track: this was previously called `Phase 8A`.
  Complete locally and retired.

The old file had `Phase 8` before the last lettered `7*` sections because UI
handoff work happened on a parallel track. That temporary numbering no longer
describes the current roadmap. In this tracker, canonical Phase 8 is Results And
Comparison.
