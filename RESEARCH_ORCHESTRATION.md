# Research Orchestration Implementation Tracker

This is the active local implementation tracker for dashboard-centered research orchestration. It is intentionally root-level while the work is unfinished. Move durable facts into `docs/` only after the system stabilizes.

## Operating Defaults

* Early work is local-first and `buba-paint`-safe.
* Remote integration may use Docker, SSH, and WSL on `testing` after the user has installed and enabled them.
* Do not implement Research UI in this track. Produce a handoff prompt for a frontend colleague after backend flows work.
* Keep the current `live_readonly` posture unchanged. Do not restart or disturb a running bot unless the user explicitly approves that phase.
* Use Compose plus inventory-driven scripts as the deploy direction.
* Use SSH-only transfer for v1 once host setup exists.
* Treat registry-pinned images as the target, with local or remote build fallback until registry setup is convenient.

## Phase Gates

Each phase stops after its verification commands and observable results. The user verifies before the next phase begins.

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

Status: in progress. Remote Compose baseline, worker heartbeat, first remote job smoke, artifact import API, remote artifact registration API, transfer lifecycle API, transfer worker execution, stale-transfer recovery, scratch archive safety, machine lifecycle API, and report regeneration API are complete.

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
* Pending: optionally switch to registry-pinned images.

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

* Add dashboard UI for artifact import, job creation, job step progress, and report browsing.

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

## Phase 8: UI Handoff Prompt

Status: complete as a functional handoff draft.

Deliverables:

* Create `docs/research-ui-handoff.md` for a frontend colleague.
* Include product goal, workflows, backend API contracts, permissions, page list, required states, chart data fixtures, mobile constraints, and acceptance tests.
* Do not implement Research UI in this track.
* Keep the handoff focused on required functionality and lifecycle coverage, not visual design.

Observable results:

* Handoff doc is complete enough for independent frontend planning.
* Missing backend CRUD and lifecycle-control endpoints are explicitly called out.

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
