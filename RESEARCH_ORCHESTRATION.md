# Research Orchestration Implementation Tracker

This is the active local implementation tracker for dashboard-centered research orchestration. It is intentionally root-level while the work is unfinished. Move durable facts into `docs/` only after the system stabilizes.

## Operating Defaults

* Current work is remote-capable but still `buba-paint`-safe.
* Remote integration uses Docker, SSH, and Ubuntu WSL on `testing`.
* Research UI is merged. Keep remaining UI cleanup narrow and reflect durable facts in `docs/` instead of temporary handoff notes.
* Treat machine identity, telemetry, and management as separate concerns.
* Machine identity comes from dashboard state plus `ops/research-machines.toml`; Docker Compose is deployment and health evidence, not the source of machine identity.
* Keep the current `live_readonly` posture unchanged. Do not restart or disturb a running bot unless the user explicitly approves that phase.
* Use Compose plus inventory-driven scripts as the deploy direction.
* Use SSH-only transfer for v1 once host setup exists.
* Use registry-pinned GHCR images for the `testing` research deploy. Keep the live deploy on the existing non-registry path until a live-specific phase exists.

## Phase Gates

Each phase stops after its verification commands and observable results. The user verifies before the next phase begins.

## Current State

The orchestration backend, remote research worker, artifact transfer path, job lifecycle controls, report lifecycle controls, machine lifecycle API, report regeneration, fixture data, and initial Research UI are implemented and merged.

The current active work is not generic machine CRUD. It is unified host observability for both the bot machine and the research machine:

* Preserve `research_machines` as durable identity and provenance state.
* Keep machine CRUD available through backend/API/scripts.
* Keep Research > Machines as a read-only observability area for research-role hosts.
* Add production-grade research host telemetry so `testing` can be observed at a similar operational depth to the existing bot Machine page.
* Surface research-host health and provenance through Research Machines, the Research Overview host list, and object details where implemented.

Open work:

* None in the current 7M-7S research observability/deploy/workflow sequence. Choose the next phase from the verified state below.

## Phase 1: Local Contracts And Tracking

Status: complete.

Deliverables:

* Create this tracker with phase checklists, defaults, and observable verification points.
* Keep UI, Docker setup, SSH setup, and Windows machine setup out of early scope.
* Confirm the plan follows repository documentation hygiene rules.

Observable results:

* `RESEARCH_ORCHESTRATION.md` exists at the repo root.
* Documentation audit passes.
* No backend, dashboard UI, deployment, or remote machine behavior changes yet.

Verification:

```bash
python3 scripts/audit-docs.py
git status --short
```

## Phase 2: Dashboard Backend Schema And APIs

Status: complete.

Deliverables:

* Extend dashboard SQLite state with research orchestration records:
  * machines
  * artifacts
  * transfers
  * jobs
  * job steps
  * job events
  * reports
* Add backend APIs for machines, artifact listing, job CRUD, job events, and reports.
* Require admin role for mutations and allow observer role for reads.
* Seed planned machine records locally:
  * `live` for `buba-paint`
  * `research` for `testing` with status `not_configured`

Observable results:

* Backend tests cover migrations, CRUD, permissions, cancellation, retry, and deterministic step creation.
* APIs work against in-memory or temporary dashboard SQLite.
* No external command execution and no remote host access.

Verification:

```bash
cargo test -p buba-dashboard
```

## Phase 3: Local Worker And Job State Machine

Status: complete.

Deliverables:

* Add a local worker boundary separate from the monitor agent.
* Execute typed allowlisted job steps only.
* Persist step transitions for queued, leased, running, blocked, retryable, failed, cancelled, and completed states.
* Append structured job events for status and progress.
* Support local execution only.

Observable results:

* Worker tests cover lease expiry, retry, cancellation, idempotent completion, event ordering, and restart behavior.
* A synthetic local job can be claimed and completed.
* No remote machine access.

Verification:

```bash
cargo test -p buba-dashboard
```

## Phase 4A: Artifact Format And Local Verification

Status: complete.

Deliverables:

* Implement artifact manifest and checksum handling.
* Restrict all file operations to configured artifact and research roots.
* Implement local artifact verification in the worker.
* Block command-backed research steps with structured state until the command executor is wired.

Observable results:

* Manifest and checksum sidecars can be written and verified.
* Unsafe artifact paths are rejected.
* Changed artifact bytes fail verification.
* The local worker can verify a job artifact and then block validation/backtest steps without running external commands.

Verification:

```bash
cargo test -p buba-dashboard
```

## Phase 4B: Command-Backed Local Research Pipeline

Status: complete.

Deliverables:

* Add a deliberate command executor boundary for `buba-paint` CLI calls.
* Parse job params for:
  * data DB path
  * start/end interval
  * prepared DB output path
  * backtest output path
  * sweep output path
  * balance
  * `--set` overrides
  * `--sweep` dimensions
* Implement local pipeline steps:
  * validate replay data
  * validate backtest input
  * prepare backtest input
  * run current-params backtest
  * run configured sweep
  * write report JSON and CSV
  * archive scratch DBs while preserving reports and manifests
* Keep command execution local-only and root-restricted.

Observable results:

* Local fake artifact with a fixture DB can run through validation gates.
* Failed validation blocks backtest and sweep execution.
* Archive refuses unsafe deletion.
* Command-backed execution is available through an explicit executor boundary. Existing tests use a fake executor; the production executor runs local `cargo run -p buba-paint --release -- ...` commands only.
* Validation command failure blocks the job before prepare, backtest, sweep, or report steps can run.

Verification:

```bash
cargo test -p buba-dashboard
cargo test -p buba-paint --test backtest_test
cargo test -p buba-paint --test sweep_test
```

## Phase 5: Safe `buba-paint` Export Integration

Status: complete.

Deliverables:

* Add export dry-run support for `buba-paint` artifacts.
* For stopped runs, checkpoint or copy DB and logs into artifact format.
* For running readonly snapshots, use SQLite backup semantics or clearly mark the artifact as a snapshot.
* Never raw-copy active WAL state.
* Keep funded `live_trading` closeout separate and stricter.

Observable results:

* Export dry run reports artifact ID, interval, source paths, estimated bytes, and safety status.
* Real export requires explicit admin action.
* Current `live_readonly` is not disturbed by default.
* Export jobs default to dry-run and do not create artifact records until `dry_run=false` and `confirm_export=true`.
* Confirmed local exports use SQLite backup for the runtime DB, copy logs by explicit path, write an artifact manifest, attach the artifact to the job, and verify it.
* Direct WAL or SHM source/log paths are rejected. Existing source WAL bytes are reported but never copied raw.
* `live_trading` export plans are blocked and remain owned by `buba-paint live-closeout`.

Verification:

```bash
cargo test -p buba-dashboard
cargo test -p buba-paint
```

## Phase 6: Local Compose And Deployment Prep

Status: complete.

Deliverables:

* Add local Compose support for dashboard backend plus worker.
* Add inventory format for future multi-machine deploys.
* Add registry image fields while preserving local or remote build fallback.
* Preserve existing `make docker-deploy` behavior.

Observable results:

* Compose config validates locally.
* Existing live Compose config still validates.
* Dry-run deploy plans show `live` and `research` targets without connecting to `testing`.
* `docker-compose.research.yml` starts only the research dashboard backend and research worker.
* The research worker binary can run once against an empty local dashboard DB and exit cleanly.
* Inventory dry-run marks `research` as a WSL-backed target after Docker and SSH setup on `testing` is ready.

Verification:

```bash
cargo test -p buba-dashboard
docker compose -f docker-compose.research.yml config --quiet
BUBA_CADDYFILE=/tmp/Caddyfile BUBA_CADDY_DATA_DIR=/tmp/buba-caddy-data BUBA_CADDY_CONFIG_DIR=/tmp/buba-caddy-config docker compose -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml config --quiet
python3 scripts/deploy-machine.py --machine live --dry-run
python3 scripts/deploy-machine.py --machine research --dry-run
```

## Phase 7: Remote Multi-Machine Integration

Status: complete through Phase 7S. Remote Compose baseline, worker heartbeat, first remote job smoke, artifact import API, remote artifact registration API, transfer lifecycle API, transfer worker execution, stale-transfer recovery, scratch archive safety, machine lifecycle API, report regeneration API, initial Research UI, shared host sampler extraction, research telemetry backend, Research Machines UI cleanup, remote telemetry smoke, registry-pinned research deploy, and browser-smoke workflow gap fixes are implemented and verified.

Deliverables:

* Complete: configure `testing` as the research worker host through Ubuntu WSL.
* Complete: configure SSH from the operator machine to `testing`, and from `testing`/WSL to `buba-paint`.
* Complete: deploy research dashboard and worker through inventory-driven Compose.
* Complete: add token-authenticated worker heartbeat into `research_machines`.
* Complete: wire dashboard-created jobs to remote worker execution on `testing`.
* Complete: replace manual artifact registration with a dashboard/API operation.
* Complete: add first-class transfer create, detail, progress, cancel, and retry controls.
* Complete: add transfer pause, resume, explicit verify, and delete-record controls.
* Complete: implement a worker transfer executor with local resumable copies and remote `rsync` over SSH.
* Complete: recover stale running transfers after worker restart so partial destinations can resume.
* Complete: archive prepared/backtest scratch DBs only after report files and report metadata are preserved.
* Complete: expose report file reads and report metadata lifecycle controls for the dashboard.
* Complete: expose artifact verification, manifest/checksum reads, metadata update, archive, restore, and guarded delete controls for the dashboard.
* Complete: expose machine create, read, update, disable, enable, guarded delete, and health controls for the dashboard.
* Complete: expose report regeneration from persisted job, step, event, and report metadata.
* Complete: merge the initial Research UI for overview, artifacts, transfers, jobs, reports, and machine management.
* Complete locally: replace the top-level Machines management page with research-host observability.
* Complete locally: persist and expose research host telemetry through typed state, sample history, and the authenticated telemetry endpoint.
* Complete remotely: switch the `testing` research deploy to registry-pinned private GHCR images.

Observable results:

* Complete: `testing` runs `research-dashboard` and `research-worker` from `/home/testing/buba-paint-research`.
* Complete: `http://localhost:3002/health` returns `{"ok":true}` inside WSL and from the Windows host.
* Complete: no-build redeploy is idempotent and keeps both containers running.
* Complete: `testing` worker can heartbeat into dashboard machine state when `BUBA_RESEARCH_CONTROLLER_URL` and `BUBA_RESEARCH_WORKER_TOKEN` are configured.
* Complete: artifact transfer resumes after interruption for local copies, recovers stale running rows after worker restart, and uses `rsync --partial --append-verify` for remote copies.
* Complete: research job runs on `testing` from dashboard-created state.
* Complete: archive failures preserve report files and a report metadata row before blocking the report step.
* Complete: report JSON, report CSV, rename, archive, restore, and delete routes are available to the authenticated dashboard.
* Complete: artifact manifest, checksum, verify, metadata update, archive, restore, and guarded delete routes are available to the authenticated dashboard.
* Complete: machine create, patch, disable, enable, health, and guarded delete routes are available to the authenticated dashboard; default `live` and `research` machines are protected from deletion.
* Complete: report regeneration is available for terminal/recoverable jobs or existing report rows without rerunning backtest or sweep commands.
* Complete: Research UI is merged in commit `bde037e`.
* Complete: research machine telemetry persists latest host identity, sampler health, worker activity, heartbeat metadata, and bounded CPU, memory, swap, disk, and load sample history; remote `testing` smoke is verified.
* Complete: Research > Machines is read-only, research-role-only, and links to telemetry detail instead of exposing machine CRUD or lifecycle controls.
* Complete remotely: `testing` Compose uses current digest-pinned private GHCR images:
  * `ghcr.io/toksaitov/buba-paint-dashboard@sha256:1bda559f8da7feb91c774fb7e4d7242073b55866e13605b976ec44eab10dcb4d`
  * `ghcr.io/toksaitov/buba-paint-research-worker@sha256:ecc49890c16a776a76feeee45decc7588d3a97f3e85d3f902798174637b0ce65`
* Complete: browser-created bounded jobs submit explicit `start_ms` and `end_ms`, completed reports can load JSON/CSV, scratch DBs can be archived from job detail, and UI cancellation terminates active child commands.

Verification:

```bash
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine research
python3 scripts/deploy-machine.py --machine research --skip-build
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
ssh testing "curl.exe -s http://localhost:3002/health"
```

### Phase 7 Smoke: Finalized Readonly Artifact

Status: complete for the first manual smoke pass.

Evidence captured on 2026-05-17:

* Finalized artifact:
  `live-readonly-20260514-184119-finalized-20260517-075706Z`
* Local artifact root:
  `data/experiments/live-readonly-20260514-184119-finalized-20260517-075706Z`
* Research-machine artifact root:
  `/home/testing/buba-paint-research/.docker/research/work/artifacts/live-readonly-20260514-184119-finalized-20260517-075706Z`
* Remote checksum verification passed:
  `remote-runtime/paint.db: OK`
* Smoke interval:
  `2026-05-17T07:39:00.000Z` to `2026-05-17T07:56:00.000Z`
* Dashboard-created job:
  `22c49fcf-780c-45dd-b6bd-8a19ecdbf172`
* Report:
  `845fee8f-5cba-4564-ab12-858a04918145`

Completed worker steps:

* `verify_artifact`
* `validate_replay_data`
* `validate_backtest_input`
* `prepare_backtest_input`
* `run_backtest`
* `write_report`

Observable results:

* `validate-replay-data` reported `sweep_grade`.
* `validate-backtest-input` reported `backtest_ready`.
* Worker prepared a 32 MB DB at:
  `/home/testing/buba-paint-research/.docker/research/work/jobs/22c49fcf-780c-45dd-b6bd-8a19ecdbf172/prepared-backtest.db`
* Worker wrote a backtest DB at:
  `/home/testing/buba-paint-research/.docker/research/work/jobs/22c49fcf-780c-45dd-b6bd-8a19ecdbf172/backtest.db`
* Worker wrote report files:
  `/home/testing/buba-paint-research/.docker/research/work/jobs/22c49fcf-780c-45dd-b6bd-8a19ecdbf172/report.json`
  `/home/testing/buba-paint-research/.docker/research/work/jobs/22c49fcf-780c-45dd-b6bd-8a19ecdbf172/report.csv`
* Dashboard API reported:
  * job status `completed`
  * all six steps `completed`
  * one artifact
  * one report
  * research machine `idle`

### Phase 7B: Backend Control-Plane API Completion

Status: complete locally. Remote smoke is the next observable check.

Deliverables:

* Add `BUBA_RESEARCH_WORK_ROOT` to the research dashboard so artifact imports are root-restricted.
* Add `POST /api/research/artifacts/import` for registering an already-local manifest artifact after checksum verification.
* Add transfer APIs for create, list, detail, progress, cancel, and retry.
* Validate artifact and machine references before creating transfers.
* Validate transfer states so bytes cannot go backwards, terminal transfers cannot be mutated into new states, and completed transfers require verified checksums.
* Keep mutation routes admin-only for the browser control plane.
* Keep inventory deploy sync source-only by excluding local `data/` and `runs/`; remote `.docker` work/runtime data remains preserved.
* Extend unit tests for import path safety, role restrictions, transfer lifecycle transitions, bad references, bad progress, cancellation, retry, and missing records.

Observable results:

* `cargo test -p buba-dashboard` passes.
* `cargo test -p buba-agent` passes.
* `cargo clippy -p buba-dashboard --all-targets -- -D warnings` passes.
* `cargo fmt --all --check` passes.
* `make comment-audit` passes.
* `python3 scripts/audit-docs.py` passes.
* `docker compose -f docker-compose.research.yml config --quiet` passes.
* Research deploy to `testing` rebuilds and restarts `research-dashboard` and `research-worker` with preserved `.docker` runtime/work data.
* Remote import smoke verifies:
  * artifact `live-readonly-20260514-184119-finalized-20260517-075706Z`
  * `files_checked=1`
  * `bytes_checked=6342098944`
  * artifact status `available`
  * source machine `live`
* Remote transfer smoke leaves one completed transfer:
  * transfer `0d39b848-480c-4c6f-baa3-77a41e1047e4`
  * source `live`
  * destination `research`
  * `bytes_done=6342098944`
  * `bytes_total=6342098944`
  * checksum status `verified`
* Remote worker transfer smoke leaves one completed `rsync` transfer:
  * transfer `b150a5b6-7c0a-4332-8913-bd7b9945fb64`
  * artifact `transfer-smoke-20260517102328`
  * source `buba-paint:/tmp/transfer-smoke-20260517102328`
  * destination `/research/artifacts/transfer-smoke-20260517102328`
  * `bytes_done=52`
  * `bytes_total=52`
  * checksum status `verified`
  * restart recovery verified by aging a stuck `running` row past `BUBA_RESEARCH_TRANSFER_STALE_MS` and letting the worker reclaim it
* Remote machine states after smoke are `live=configured` and `research=idle`.

Remaining Phase 7 implementation work:

* Dashboard UI for artifact import, job creation, job step progress, and report browsing: merged in commit `bde037e`.
* The current `/research/*` routes in `dashboard/client/src/App.tsx` cover Overview, Machines, Artifacts, Transfers, Jobs, and Reports lists plus detail pages.
* Lifecycle controls follow `dashboard/client/src/lib/research-permissions.ts`.
* Host-observability cleanup is complete locally: Research > Machines is now a research-host telemetry area, and machine records remain shared lookup, source/destination, health, and provenance data for the other Research pages.
* Registry-pinned deploy cleanup is complete for the `testing` research stack. The live stack remains on the existing non-registry deploy path.

### Phase 7C: Worker Transfer Executor

Status: complete. Local tests and remote `testing` worker smoke have passed.

Deliverables:

* Add `dashboard/server/src/research_transfer.rs`.
* Let the research worker claim queued or retryable transfer records for its local machine.
* Recover stale running transfer records for the local machine before claiming new work.
* Copy same-machine artifacts with append-style file resume.
* Copy remote artifacts with `rsync -a --partial --append-verify --compress --protect-args --stats` over SSH.
* Verify the destination manifest before marking a transfer complete.
* Update the artifact row to point at the local research artifact root after successful transfer.
* Mark failed transfer attempts as `retryable` with checksum status `failed`.
* Install `rsync` and `openssh-client` in the research worker image.
* Mount the research host SSH directory into the worker container at `/home/buba/.ssh`.

Observable results:

* Unit tests cover local copy, partial resume, stale running recovery, retryable failure, transfer claiming, and `rsync` command construction.
* `cargo test -p buba-dashboard` passes.
* `cargo clippy -p buba-dashboard --all-targets -- -D warnings` passes.
* `make comment-audit` passes.
* `python3 scripts/audit-docs.py` passes.
* `docker compose -f docker-compose.research.yml config --quiet` passes.
* `python3 scripts/deploy-machine.py --machine research` rebuilds and restarts the remote stack.
* Remote worker transfers from `buba-paint` to `testing` complete through `rsync`, verify checksums, update artifact metadata to `/research/artifacts/...`, and recover a stale `running` row after worker restart.

### Phase 7D: Operator API Completion

Status: complete. Local tests and remote `testing` smoke without manual SQLite edits have passed.

Deliverables:

* Add `POST /api/research/artifacts/register` for remote-source manifest metadata registration.
* Validate remote artifact roots, artifact IDs, manifest schema, file paths, checksums, and source-machine references before storing metadata.
* Default transfer source machine and byte totals from artifact metadata when a transfer is created.
* Add `POST /api/research/transfers/{id}/pause`.
* Add `POST /api/research/transfers/{id}/resume`.
* Add `POST /api/research/transfers/{id}/verify`.
* Add `DELETE /api/research/transfers/{id}`.
* Keep all artifact and transfer mutations admin-only.

Observable results:

* Unit tests cover remote artifact registration, bad remote metadata, source-machine mismatches, transfer source/byte defaults, pause, resume, verify, delete, role restrictions, missing records, and invalid transitions.
* `cargo test -p buba-dashboard api::research -- --nocapture` passes.
* `cargo test -p buba-dashboard artifact_transfer -- --nocapture` passes.
* Remote no-SQLite registration and transfer smoke leaves one completed transfer:
  * artifact `register-smoke-20260517105413`
  * registered source root `/tmp/register-smoke-20260517105413`
  * transfer `99033830-8ca5-4962-8194-cb7506b05016`
  * destination `/research/artifacts/register-smoke-20260517105413`
  * `bytes_done=54`
  * `bytes_total=54`
  * checksum status `verified`
  * explicit transfer verify status `completed` with checksum `verified`

### Phase 7E: Scratch Archive Safety

Status: complete locally and on `testing` smoke.

Deliverables:

* Persist report JSON, report CSV, and the `research_reports` metadata row before any scratch deletion runs.
* Keep archive deletion limited to prepared/backtest SQLite families under the job root.
* Preserve report files and report metadata when archive validation or deletion fails.
* Add archive failure details into report JSON under `archive_error` before blocking the `write_report` step.

Observable results:

* Successful archive writes an `archive` summary into report JSON and report metadata.
* Failed archive leaves report files on disk, leaves the report metadata searchable, and marks the job blocked for operator recovery.
* Remote archive smoke on `testing` completed job `b9a667a1-61cc-4ff2-81e3-356af0b4c23d`, kept report `bbb69499-bc6a-4ab4-bcc4-11db8a81f377`, deleted two scratch DB files, and skipped four absent sidecars.

### Phase 7F: Report Lifecycle API

Status: complete locally and on `testing` smoke.

Deliverables:

* Add `PATCH /api/research/reports/{id}` for title and lifecycle status updates.
* Add archive and restore controls for report metadata.
* Add `DELETE /api/research/reports/{id}` with explicit `delete_files=true` opt-in for file deletion.
* Add `GET /api/research/reports/{id}/json` and `/csv` file reads, restricted to the configured research work root.
* Keep report mutations admin-only while allowing authenticated report reads.

Observable results:

* Unit tests cover file reads, metadata update, archive, restore, delete with files, observer restrictions, and unsafe stored report paths.
* File deletion refuses stored paths outside the research work root and leaves metadata intact on rejection.
* Remote smoke on `testing` read report JSON and CSV for `bbb69499-bc6a-4ab4-bcc4-11db8a81f377`, patched and restored the title, archived and restored the report, and left it `available`.

### Phase 7G: Artifact Lifecycle API

Status: complete locally and on `testing` smoke.

Deliverables:

* Add `PATCH /api/research/artifacts/{id}` for source/run-mode and quality metadata corrections.
* Add `POST /api/research/artifacts/{id}/verify` for local manifest and checksum verification.
* Add archive and restore controls for artifact metadata.
* Add `GET /api/research/artifacts/{id}/manifest` and `/checksums` for dashboard inspection.
* Add `DELETE /api/research/artifacts/{id}` with dependency checks and explicit `delete_files=true` opt-in for local file deletion.
* Keep artifact mutations admin-only while allowing authenticated artifact reads.

Observable results:

* Unit tests cover metadata update, manifest read, checksum read, verification, archive, restore, delete with files, observer restrictions, and file deletion.
* Artifact deletion is rejected when jobs, reports, or transfers still reference the artifact.
* Remote smoke on `testing` read manifest and checksum text for `live-readonly-20260514-184119-finalized-20260517-075706Z`, verified one 6.34 GB payload, performed a same-value metadata patch, archived and restored the artifact, and left artifact metadata unchanged with final status `available`.

### Phase 7H: Job Lifecycle API

Status: complete locally and on `testing` smoke.

Deliverables:

* Add `PATCH /api/research/jobs/{id}` for queued job priority, params, and artifact corrections before any step has started.
* Add durable `pause`, `resume`, and `continue` controls for jobs.
* Keep resume/continue compatible with retry semantics so completed steps are preserved.
* Add `POST /api/research/jobs/{id}/clone` to create a fresh queued job from prior type, artifact, params, and optional overrides.
* Keep all job lifecycle mutations admin-only while preserving authenticated job reads.

Observable results:

* Unit tests cover queued-job updates, artifact clearing rules, update rejection after step execution starts, pause/resume lease blocking, cancelled-job continuation, clone provenance events, observer restrictions, and invalid mutations.
* Paused jobs are not leased by the worker because the step lease query only considers queued/running/retryable jobs.
* Remote smoke on `testing` created two temporary low-priority jobs, exercised create, patch, pause, resume, cancel, continue, clone, clone cancel, and final cancel with HTTP 200 responses, then removed both temporary rows (`jobs_after_cleanup = 0`).

### Phase 7I: Step Recovery API

Status: complete locally and on `testing` smoke.

Deliverables:

* Add `POST /api/research/jobs/{job_id}/steps/{step_id}/retry` for failed, blocked, retryable, cancelled, or paused steps.
* Add `POST /api/research/jobs/{job_id}/steps/{step_id}/cancel` for non-terminal steps, marking the owning job cancelled.
* Add `POST /api/research/jobs/{job_id}/steps/{step_id}/clear-lease` for expired active leases only.
* Add `POST /api/research/jobs/{job_id}/steps/{step_id}/resolve-blocker` for blocked steps after an operator action.
* Keep step mutations admin-only and return refreshed job detail after every step operation.

Observable results:

* Unit tests cover step retry, blocker resolution, individual step cancellation, cancelled-step continuation, stale lease clearing, early lease-clear rejection, observer restrictions, and refreshed job details.
* Step skip is intentionally not implemented because there are no optional safe-to-skip step templates yet.
* Remote smoke on `testing` inserted two temporary low-priority jobs with controlled step states, exercised retry, resolve-blocker, cancel, retry-after-cancel, and clear-stale-lease with HTTP 200 responses, then removed both temporary rows (`jobs_after_cleanup = 0`).

### Phase 7J: Job Delete API

Status: complete locally and deployed to `testing`.

Deliverables:

* Add `DELETE /api/research/jobs/{id}` for inactive jobs.
* Reject deletion of queued or running jobs.
* Reject deletion of jobs that still have reports.
* Delete job events and steps with the job metadata row while leaving artifacts and reports untouched.
* Keep job deletion admin-only.

Observable results:

* Unit tests cover active-job rejection, reported-job rejection, event/step cleanup, admin deletion, observer restriction, and missing-job behavior.
* Remote smoke on `testing` created a temporary low-priority export job, confirmed active delete is rejected with HTTP 400, cancelled it, deleted it with HTTP 200, confirmed follow-up fetch returns HTTP 404, and verified zero smoke leftovers in SQLite.

### Phase 7K: Machine Lifecycle API

Status: complete locally and deployed to `testing`.

Deliverables:

* Add `POST /api/research/machines` for custom machine registration.
* Add `GET`, `PATCH`, and `DELETE /api/research/machines/{id}`.
* Add `POST /api/research/machines/{id}/disable` and `/enable`.
* Add `GET /api/research/machines/{id}/health` with parsed details and dependency counts.
* Reject invalid machine IDs, roles, statuses, empty metadata, and invalid details JSON.
* Preserve `disabled` status across worker heartbeats and prevent disabled machines from claiming new transfer work.
* Reject deletion of default machines and machines still referenced by artifacts, transfers, jobs through source artifacts, or reports through source artifacts.
* Keep machine mutations admin-only while allowing authenticated machine reads.

Observable results:

* Unit tests cover custom machine create/update/delete, invalid records, default/referenced delete guards, disabled heartbeat preservation, disabled transfer claim blocking, admin API lifecycle controls, observer restrictions, and health output.
* Remote smoke on `testing` created a temporary custom machine, patched metadata, read health, disabled it, confirmed a worker heartbeat preserves `disabled`, enabled it, deleted it, confirmed follow-up fetch returns HTTP 404, and verified zero smoke leftovers in SQLite.

### Phase 7L: Report Regeneration API

Status: complete locally and deployed to `testing`.

Deliverables:

* Add `POST /api/research/jobs/{id}/report/regenerate`.
* Rebuild report JSON and step-summary CSV from durable job, step, event, and report metadata only.
* Do not rerun backtest, sweep, export, or validation commands.
* Use existing report paths when present; otherwise write default files under the configured research work root.
* Reject queued or otherwise unready jobs when no report row already exists.
* Reject unsafe stored report paths outside the research work root.
* Keep regeneration admin-only.

Observable results:

* Unit tests cover successful regeneration, observer rejection, unready-job rejection, unsafe existing path rejection, file creation, and persisted report metadata.
* Remote smoke on `testing` created a temporary completed export job state, regenerated JSON/CSV through the API, read both files through report file endpoints, deleted the generated report with files, deleted the temporary job, and verified zero smoke leftovers in SQLite.

### Phase 7M: Unified Host Observability And Machines UI Cleanup

Status: complete. 7M.1 shared sampler, 7M.2 research telemetry backend, 7M.3 Machines UI cleanup, and 7M.4 remote telemetry smoke are implemented and verified.

Problem:

* The bot Machine page is backed by a real host sampler in `agent/src/machine.rs`.
* The research worker now records typed host telemetry locally and can optionally publish the same payload to a controller dashboard.
* The initial merged Research Machines page exposed infrastructure CRUD as a primary Research page, but the product need is operational host observability, not day-to-day machine management.

Design direction:

* Treat machine identity, host telemetry, and machine management as separate concerns.
* Keep `research_machines` and `ops/research-machines.toml` as machine identity and provenance sources.
* Keep machine lifecycle APIs for scripts, admin recovery, and future multi-worker registration.
* Add first-class host telemetry for `testing` through the research worker.
* Keep Research > Machines in navigation as research-host observability, not management.
* Surface research-host health and provenance in object detail pages without forcing CRUD navigation.

Deliverables:

* Complete: extract the shared `buba-machine-telemetry` crate so bot and research hosts use one telemetry contract.
* Complete locally: add host sampling to `buba-research-worker` for CPU, per-core CPU, load, memory, swap, work-root disk, sampler health, and worker status.
* Complete locally: include worker activity context in research telemetry for disabled state, processed job steps, processed transfers, configured worker limits, heartbeat interval, and last loop error.
* Complete locally: persist research host samples in `research_machine_telemetry_state` and `research_machine_telemetry_samples` instead of storing only arbitrary heartbeat details.
* Complete locally: add `GET /api/research/machines/:id/telemetry` for authenticated dashboard reads.
* Complete locally: update seed fixtures so Research UI tests can cover healthy, stale, loaded, low-disk, disabled, and no-telemetry hosts.
* Complete locally: replace Research Machines as a top-level management page with research-host telemetry list and detail pages.
* Complete locally: render artifact and transfer source/destination machine references as telemetry links only for research-role hosts, and as provenance labels otherwise.
* Complete locally: keep machine create/update/delete/disable/enable API coverage while telemetry backend remains separate from management.
* Complete locally: update docs to describe machine identity versus telemetry versus management.
* Complete remotely: keep non-registry research deploys on an isolated remote `DOCKER_CONFIG` so SSH-driven Docker Desktop builds do not depend on an interactive credential helper session.

Observable results:

* Complete locally: Bot Machine page keeps working with the shared sampler-backed agent API.
* Complete locally: research telemetry API returns current state, bounded sample history, dependency counts, disabled state, stale state, and stale threshold.
* Complete locally: Research Machines list and detail pages show telemetry health for research-role hosts when telemetry exists.
* Complete locally: Research UI still works when no research telemetry exists, and marks the state as missing or stale rather than inventing health.
* Complete locally: Artifact and transfer detail views show machine context without requiring a Machines management page.
* Complete locally: a disabled research machine does not claim work and still updates telemetry state.
* Complete locally: high CPU, low memory, swap pressure, low disk, stale heartbeat, sampler errors, and worker errors produce operator-visible UI warnings.
* Complete locally: machine CRUD remains tested at the API level but is no longer a primary Research navigation destination.
* Complete remotely: `testing` runs `research-dashboard` and `research-worker` through `docker-compose.research.yml`; `/health` returns ok.
* Complete remotely: authenticated telemetry for `research` is fresh, non-stale, and includes worker state plus CPU, memory, swap, disk, load, host identity, and sampler health fields from real `testing` samples.
* Complete remotely: Research > Machines and Research > Machines > research render through a temporary SSH tunnel with Host, CPU, Memory & Swap, Disk, and Worker sections and without machine CRUD controls.
* Complete remotely: `buba-paint` container IDs and uptime remained unchanged during 7M.4.

Verification:

```bash
cargo fmt --all --check
cargo test -p buba-agent
cargo test -p buba-dashboard
cd dashboard/client && npm test
python3 scripts/audit-docs.py
docker compose -f docker-compose.research.yml config --quiet
python3 scripts/deploy-machine.py --machine research --dry-run
```

Remote smoke target:

```bash
python3 scripts/deploy-machine.py --machine research
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'cd /home/testing/buba-paint-research && docker compose -f docker-compose.research.yml ps'"
ssh testing "wsl -d Ubuntu-24.04 -- bash -lc 'curl -sf http://localhost:3002/health'"
```

### Phase 7Q: Frontend Lint Debt Cleanup

Status: complete locally.

Deliverables:

* Complete: make `cd dashboard/client && npm run lint` pass with zero errors and zero warnings.
* Complete: remove unused frontend test symbols without changing test intent.
* Complete: replace empty frontend catches with explicit no-op or fallback returns.
* Complete: refactor media-query, app-shell, header, signal-filter, and trade-filter state so lint-clean code preserves existing behavior.
* Complete: split the shared test provider from `renderWithProviders` so fast-refresh lint stays clean.

Observable results:

* Complete: frontend lint is a passing gate before 7M.4 remote smoke.
* Complete: protected-route auth behavior, bot selection fallback, mobile header behavior, media-query updates, persisted filters, and websocket invalid-message tolerance remain covered by existing tests.

Verification:

```bash
cd dashboard/client && npm run lint
cd dashboard/client && npm test
cd dashboard/client && npm run build
cd dashboard/client && npm run test:e2e -- --project=chromium
python3 scripts/audit-docs.py
git diff --check
```

### Phase 7R: Registry-Pinned Research Deployment

Status: complete. Local publishing, digest lock validation, registry-pinned research deploy, authenticated telemetry, UI smoke, and live safety checks passed.

Deliverables:

* Complete: add a local research image publisher that builds dashboard and research-worker images, pushes private GHCR tags, resolves digest refs, and writes `ops/research-images.lock.json`.
* Complete: use digest refs from the lock file for `testing` research deploys.
* Complete: keep `live` on the existing non-registry path.
* Complete: sync only research deployment files for registry-pinned research deploys.
* Complete: authenticate remote GHCR pulls with a temporary Docker config and the current local `gh` token over SSH; remove the config before the session exits.
* Complete: keep non-registry fallback behavior for local or remote builds.

Observable results:

* Complete: `python3 scripts/publish-research-images.py --dry-run` reports planned GHCR images, source input hashes, and package-scope readiness.
* Complete: `python3 scripts/deploy-machine.py --machine research --dry-run` fails when the image lock is missing or stale and passes once the lock matches current image inputs.
* Complete: `python3 scripts/deploy-machine.py --machine live --dry-run` remains non-registry and does not require the research lock.
* Complete: remote research deploy pulls digest-pinned GHCR images and does not build on `testing`.
* Complete: `buba-paint` remains untouched except for before and after status snapshots.
* Complete: published linux/amd64 image refs:
  * `ghcr.io/toksaitov/buba-paint-dashboard@sha256:c1381019ab5978f00897790f2ade2deaa9d48e18f1762cf00c5569568ed72d0e`
  * `ghcr.io/toksaitov/buba-paint-research-worker@sha256:e9290e9050807922e3ac7b6727685d6407dc8957a57946267fd0e3605009ff25`
* Complete: `testing` Compose uses those digest refs for `research-dashboard` and `research-worker`.
* Complete: authenticated telemetry for `research` was fresh and non-stale with 60 samples, worker `research-worker-testing`, CPU, memory, swap, disk, load, host identity, and sampler interval fields.
* Complete: Research > Machines UI smoke through a temporary SSH tunnel rendered the research host list and detail page with Host, CPU, Memory & Swap, Disk, and Worker sections and no machine CRUD controls.
* Complete: `buba-paint` before and after snapshots kept container IDs unchanged: `50003527ee0e`, `2deb6c87a582`, and `773286fe7eb7`.

Verification:

```bash
python3 scripts/publish-research-images.py --dry-run
gh auth status
docker compose -f docker-compose.research.yml config --quiet
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine live --dry-run
python3 scripts/deploy-machine.py --machine research
python3 scripts/audit-docs.py
git diff --check
```

### Phase 7S: End-To-End Research Workflow Smoke And Gap Fixes

Status: complete.

Goal:

* Prove the real operator workflow on `testing` through the browser before adding more deployment machinery.
* Use the finalized live-readonly artifact already present on the research host.
* Keep `buba-paint` untouched.
* Record and fix workflow gaps that would make the dashboard unsafe or surprising for a real research run.

Initial browser-controlled smoke results:

* Complete: Research Overview, Artifacts, Jobs, Reports, and Machines loaded through the `127.0.0.1:3302` tunnel without fetch errors.
* Complete: the browser-created job flow reached the job detail page and showed live step progress.
* Complete: the oversized browser-created jobs were cancelled from the UI:
  * `6b4f3a7b-92fe-422e-800d-04d91ca2b494`
  * `b2bea74e-8198-4442-bf77-935c89850df1`
* Complete: a bounded current-params backtest was created through the authenticated API after the browser form gap was found:
  * job `4c57686a-b0c7-4a36-81f8-ed36fbea55f7`
  * report `565561ba-05d6-4dd0-9dce-963399084018`
  * interval `2026-05-17T07:39:00Z` to `2026-05-17T07:41:00Z`
* Complete: the bounded job finished all six steps:
  * `verify_artifact`
  * `validate_replay_data`
  * `validate_backtest_input`
  * `prepare_backtest_input`
  * `run_backtest`
  * `write_report`
* Complete: the report detail page loaded both JSON and CSV through the browser.
* Complete: Research > Machines showed fresh non-stale telemetry and returned to `Idle` after the bounded job.

Gaps found:

* The New Job form is too easy to misuse. Explicit browser-filled Start and End values did not affect the request in the smoke, and both browser-created jobs fell back to the full artifact interval. At minimum, the UI needs a visible effective interval preview and stronger validation before submitting a multi-day artifact interval.
* Job cancellation updates dashboard state immediately, but it does not interrupt an already-running `buba-paint` child process. Both oversized smoke jobs required manual child-process termination on `testing`; the worker container restarted after the manual termination.
* Completed current-params jobs still leave scratch DBs in the job directory. For job `4c57686a-b0c7-4a36-81f8-ed36fbea55f7`, `prepared-backtest.db` and `backtest.db` remained next to `report.json` and `report.csv`. The dashboard needs an explicit archive or cleanup path that preserves reports and manifests while deleting scratch DBs.

Fixes implemented:

* Complete: New Job shows effective Start, End, duration, and source for current-params and sweep jobs.
* Complete: explicit browser-filled `datetime-local` values are reflected in the effective interval and submitted as `start_ms` and `end_ms`.
* Complete: invalid, missing, reversed, fallback-derived, and large intervals gate Create appropriately; large and fallback-derived intervals require confirmation.
* Complete: active `buba-paint` child commands are supervised. Job cancellation terminates the child process, records a cancellation event, and keeps the worker alive.
* Complete: `POST /api/research/jobs/:id/archive-scratch` deletes only prepared/backtest scratch SQLite families under the job root and preserves reports, artifacts, manifests, and report metadata.
* Complete: Job detail exposes `Archive scratch DBs` only for completed jobs with reports and shows a deleted/skipped summary after confirmation.

Final browser-controlled smoke results:

* Complete: published and deployed digest-pinned research images to `testing`:
  * dashboard `ghcr.io/toksaitov/buba-paint-dashboard@sha256:1bda559f8da7feb91c774fb7e4d7242073b55866e13605b976ec44eab10dcb4d`
  * worker `ghcr.io/toksaitov/buba-paint-research-worker@sha256:ecc49890c16a776a76feeee45decc7588d3a97f3e85d3f902798174637b0ce65`
* Complete: through the browser, created bounded current-params job `f8b4e4a5-5055-4345-9f2a-e89843ea4b2f` from artifact `live-readonly-20260514-184119-finalized-20260517-075706Z`.
  * Effective interval preview showed explicit input from `2026-05-17 13:39:00 GMT+6` to `2026-05-17 13:41:00 GMT+6`.
  * Job detail persisted `start_ms=1779003540000` and `end_ms=1779003660000`.
  * All six steps completed and report `4d330f43-2b3d-4be2-8c3c-ca49f8fed1b1` was generated.
* Complete: report detail loaded both JSON and CSV through browser controls.
* Complete: `Archive scratch DBs` ran from the job detail page and reported 2 deleted scratch files and 4 skipped sidecars.
  * Remote filesystem confirmed `prepared-backtest.db` and `backtest.db` were gone.
  * Remote filesystem confirmed `report.json` and `report.csv` remained.
* Complete: through the browser, created large explicit job `1e0b42ea-fdb3-4776-8655-23ec10df4530`, confirmed the large interval, waited until `validate_replay_data` was running, and cancelled from the job detail page.
  * Worker recorded `research command terminated after cancellation`.
  * Job and downstream steps remained `cancelled`; no failed/blocked overwrite occurred.
  * `docker top buba-paint-research-research-worker-1` showed only `buba-research-worker`; no `buba-paint` child remained.
* Complete: Research > Machines returned to `Idle`, telemetry stayed non-stale, and the latest telemetry had 60 samples.
* Complete: `buba-paint` was not touched. Before and after snapshots kept container IDs unchanged: `50003527ee0e`, `2deb6c87a582`, and `773286fe7eb7`.

Known smoke note:

* Job `7c8e48df-bb5e-4323-9f50-63d06a0db5e7` intentionally remains as blocked smoke evidence. It used a 10-minute interval that passed validation but failed `prepare_backtest_input` due a missing boundary price. The successful final smoke used the previously verified 2-minute interval.

Verification:

```bash
cargo fmt --all --check
cargo test -p buba-dashboard
cd dashboard/client && npm test
cd dashboard/client && npm run build
python3 scripts/audit-docs.py
python3 scripts/publish-research-images.py --dry-run
python3 scripts/publish-research-images.py
python3 scripts/deploy-machine.py --machine research --dry-run
python3 scripts/deploy-machine.py --machine research
git diff --check
```

## Phase 8: UI Handoff And Merge

Status: retired after UI merge.

Deliverables:

* Created a temporary functional handoff for a frontend colleague.
* Captured product goal, workflows, backend API contracts, permissions, page list, required states, fixture data, environment context, and acceptance tests.
* Keep the handoff focused on required functionality and lifecycle coverage, not visual design.
* Delete the temporary handoff doc after UI merge and move durable facts into stable docs.

Observable results:

* Research dashboard UI is merged in commit `bde037e`.
* Temporary UI handoff document was removed.
* Durable Research UI facts now live in `docs/system-architecture.md`.
* Remaining Machines-page cleanup stays tracked in this implementation file until the route is removed or demoted.

Verification:

```bash
python3 scripts/audit-docs.py
```

### Phase 8A: Research UI Fixtures

Status: complete locally.

Deliverables:

* Add `scripts/seed-research-fixtures.py`.
* Seed representative `fixture-` machines, artifacts, transfers, jobs, steps, events, and reports.
* Write manifest/checksum sidecars, report JSON, and report CSV under a caller-provided research work root.
* Include available, archived, missing-file, checksum-failure, partial-transfer, retryable, disabled-machine, completed, blocked, failed, cancelled, running, and paused states.
* Keep the seeder opt-in and safe: `--reset` only removes rows whose IDs start with `fixture-` and fixture files under the provided work root.

Observable results:

* A temp smoke run created 3 machines, 3 artifacts, 4 transfers, 6 jobs, 34 steps, 6 events, and 3 reports.
* Running the seeder a second time without `--reset` failed with a clear duplicate-fixture error.
* Running again with `--reset` recreated the same fixture counts.

Verification:

```bash
python3 -m py_compile scripts/seed-research-fixtures.py
tmpdir=$(mktemp -d)
python3 scripts/seed-research-fixtures.py --db "$tmpdir/dashboard.db" --work-root "$tmpdir/work" --reset
python3 scripts/seed-research-fixtures.py --db "$tmpdir/dashboard.db" --work-root "$tmpdir/work" --reset
```
