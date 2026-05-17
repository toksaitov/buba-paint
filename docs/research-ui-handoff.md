# Research UI Functional Handoff

This file is a functional product and API handoff for the frontend colleague who will design and implement the Research dashboard. It intentionally avoids visual layout direction. The colleague should decide presentation, information hierarchy, interaction details, and styling using the existing dashboard system.

## Non-Negotiable Principle

The Research UI is not complete unless every long-running entity has full lifecycle operations. Listing things is not enough.

For every entity that can be created or started, the user must be able to inspect it, edit allowed fields, stop/cancel it, continue/resume it when safe, retry it after failure, archive/delete it when appropriate, and understand exactly why an operation is disabled.

## Product Goal

The dashboard should centrally control the research workflow:

1. End or export a live/readonly run without disturbing active trading posture.
2. Package run data into a verifiable artifact.
3. Transfer or stage that artifact for research execution.
4. Create current-params backtests and sweep jobs.
5. Watch every step, log, event, report, transfer, and failure.
6. Resume or retry safely after partial failure.
7. Archive scratch data after reports are preserved.
8. Keep a searchable history of runs, artifacts, jobs, transfers, and reports.

## Current Backend Reality

The backend has local-first research state, worker execution, artifact import, transfer lifecycle state, and worker-side transfer execution. It is enough to build read-only screens, artifact import flows, transfer progress/control surfaces, and basic job creation/control, but not enough for a complete UI.

Available HTTP endpoints:

| Capability | Endpoint | Current state |
| --- | --- | --- |
| List machines | `GET /api/research/machines` | Available |
| Create machine | `POST /api/research/machines` | Available, admin only |
| Read machine | `GET /api/research/machines/{id}` | Available |
| Update machine | `PATCH /api/research/machines/{id}` | Available, admin only |
| Disable machine | `POST /api/research/machines/{id}/disable` | Available, admin only |
| Enable machine | `POST /api/research/machines/{id}/enable` | Available, admin only |
| Delete machine | `DELETE /api/research/machines/{id}` | Available for custom unreferenced machines, admin only |
| Machine health | `GET /api/research/machines/{id}/health` | Available |
| Worker heartbeat | `POST /api/research/workers/heartbeat` | Available; worker token only |
| List artifacts | `GET /api/research/artifacts` | Available |
| Import local artifact | `POST /api/research/artifacts/import` | Available, admin only |
| Register remote artifact | `POST /api/research/artifacts/register` | Available, admin only |
| Read artifact | `GET /api/research/artifacts/{id}` | Available |
| List transfers | `GET /api/research/transfers` | Available |
| Create transfer record | `POST /api/research/transfers` | Available, admin only |
| Read transfer | `GET /api/research/transfers/{id}` | Available |
| Update transfer progress | `POST /api/research/transfers/{id}/progress` | Available, admin only |
| Cancel transfer | `POST /api/research/transfers/{id}/cancel` | Available, admin only |
| Pause transfer | `POST /api/research/transfers/{id}/pause` | Available, admin only |
| Resume transfer | `POST /api/research/transfers/{id}/resume` | Available, admin only |
| Retry transfer | `POST /api/research/transfers/{id}/retry` | Available, admin only |
| Verify transfer | `POST /api/research/transfers/{id}/verify` | Available, admin only |
| Delete transfer record | `DELETE /api/research/transfers/{id}` | Available, admin only |
| List jobs | `GET /api/research/jobs` | Available |
| Create job | `POST /api/research/jobs` | Available, admin only |
| Read job detail | `GET /api/research/jobs/{id}` | Available; includes steps and events |
| Cancel job | `POST /api/research/jobs/{id}/cancel` | Available, admin only |
| Retry job | `POST /api/research/jobs/{id}/retry` | Available, admin only |
| List job events | `GET /api/research/jobs/{id}/events` | Available |
| Append job event | `POST /api/research/jobs/{id}/events` | Available, admin only |
| List reports | `GET /api/research/reports` | Available |
| Read report metadata | `GET /api/research/reports/{id}` | Available |

Current DB entities:

| Entity | Purpose |
| --- | --- |
| `research_machines` | Live and research hosts, including `buba-paint` and `testing`. |
| `run_artifacts` | Exported run data packages with manifest/checksum metadata. |
| `artifact_transfers` | Transfer state and progress records claimed by the research worker. |
| `research_jobs` | Export, current-params backtest, and sweep jobs. |
| `research_job_steps` | Durable ordered step state for each job. |
| `research_job_events` | Timeline/progress/audit events. |
| `research_reports` | Generated report metadata and report file paths. |

Current job types:

| Job type | Required input | Step sequence |
| --- | --- | --- |
| `export` | `params.source_db_path`; optional logs/interval/run mode/dry-run flags | `plan_export`, `snapshot_or_copy_runtime`, `write_artifact_manifest`, `verify_artifact` |
| `current_params` | `artifact_id`, interval params or artifact interval | `verify_artifact`, `validate_replay_data`, `validate_backtest_input`, `prepare_backtest_input`, `run_backtest`, `write_report` |
| `sweep` | `artifact_id`, interval params or artifact interval, at least one sweep dimension | `verify_artifact`, `validate_replay_data`, `validate_backtest_input`, `prepare_backtest_input`, `run_sweep`, `write_report` |

Current terminal and transient states include:

`queued`, `leased`, `running`, `paused`, `completed`, `blocked`, `retryable`, `failed`, `cancelled`.

## Seeded UI Fixtures

Use the fixture seeder when building or testing the Research UI against a local or disposable dashboard database:

```bash
python3 scripts/seed-research-fixtures.py \
  --db /path/to/dashboard.db \
  --work-root /tmp/buba-research-fixtures \
  --reset
```

The seeder only touches rows whose IDs start with `fixture-`, and `--reset` removes and recreates those rows plus fixture files under the supplied work root. Running without `--reset` fails if fixture rows already exist, which prevents accidental duplicate states.

Seeded machines:

| ID | State covered |
| --- | --- |
| `fixture-live` | Configured live/source host. |
| `fixture-research` | Idle research worker host. |
| `fixture-disabled` | Disabled worker that should not receive new work. |

Seeded artifacts and transfers:

| ID | State covered |
| --- | --- |
| `fixture-artifact-available` | Available manifest-backed readonly artifact. |
| `fixture-artifact-archived` | Archived artifact metadata. |
| `fixture-artifact-bad-checksum` | Manifest checksum mismatch. |
| `fixture-transfer-running` | Partial running transfer. |
| `fixture-transfer-retryable` | Partial failed/retryable transfer with error text. |
| `fixture-transfer-paused` | Paused transfer. |
| `fixture-transfer-completed` | Completed and verified transfer. |

Seeded jobs and reports:

| ID | State covered |
| --- | --- |
| `fixture-job-completed` | Completed current-params job with an available report. |
| `fixture-job-blocked` | Blocked validation step with queued downstream steps. |
| `fixture-job-failed` | Failed sweep step with an archived report row. |
| `fixture-job-cancelled` | Cancelled export job with completed prior work. |
| `fixture-job-running` | Leased/running first step. |
| `fixture-job-paused` | Paused validation step. |
| `fixture-report-available` | JSON and CSV files exist and include chart-friendly metrics. |
| `fixture-report-archived` | Archived report metadata with preserved files. |
| `fixture-report-missing-file` | Metadata exists but JSON/CSV files are absent. |

## Required Backend Gaps Before UI Completion

The UI work should not hide these missing operations. If the backend does not expose them yet, create explicit backend tickets or stub the controls as unavailable with a clear reason.

### Machines

Required operations:

| Operation | Need |
| --- | --- |
| Create/register machine | Available for custom hosts beyond seeded `live` and `research`. |
| Read/list machines | Already available. |
| Update machine | Available for name, role, SSH alias, status, and structured details. |
| Disable machine | Available. Preserves disabled state across worker heartbeats and prevents new transfer claims; running work remains best-effort. |
| Delete machine | Available only for custom machines with no dependent artifacts, transfers, jobs through source artifacts, or reports through source artifacts. Default `live` and `research` are disabled instead of deleted. |
| Health/heartbeat | Available with parsed details, status, disabled flag, and dependency counts. Worker-provided telemetry is stored in `details_json`. |

Available API endpoints:

```text
POST /api/research/machines
GET /api/research/machines/{id}
PATCH /api/research/machines/{id}
POST /api/research/machines/{id}/disable
POST /api/research/machines/{id}/enable
DELETE /api/research/machines/{id}
GET /api/research/machines/{id}/health
```

Available worker heartbeat:

```text
POST /api/research/workers/heartbeat
```

This endpoint is not a browser operation. It uses `BUBA_RESEARCH_WORKER_TOKEN`, updates `research_machines.status`, and stores `worker_id`, `worker_version`, `last_heartbeat_ms`, heartbeat status, and optional telemetry in `details_json`.

### Artifacts

Required operations:

| Operation | Need |
| --- | --- |
| Create/register artifact | Local manifest import is available through `POST /api/research/artifacts/import`; remote source metadata registration is available through `POST /api/research/artifacts/register`; worker-created artifacts are available through export jobs. |
| Read/list artifact | Already available. |
| Update metadata | Quality class overrides and source attribution corrections are available. Labels and notes still need schema support. |
| Verify artifact | Available for local artifact roots. |
| Archive artifact | Available. |
| Delete artifact | Available only when no jobs, reports, or transfers reference the artifact; file deletion requires `delete_files=true`. |
| Restore/unarchive artifact | Available. |
| Download/view manifest | Manifest and checksum text endpoints are available for local artifact roots. |

Available API endpoints:

```text
PATCH /api/research/artifacts/{id}
POST /api/research/artifacts/{id}/verify
POST /api/research/artifacts/{id}/archive
POST /api/research/artifacts/{id}/restore
DELETE /api/research/artifacts/{id}
GET /api/research/artifacts/{id}/manifest
GET /api/research/artifacts/{id}/checksums
```

### Transfers

Required operations:

| Operation | Need |
| --- | --- |
| Create transfer | Available as a record operation. The research worker can execute queued transfers for its machine. |
| Read/list transfer | Already available. |
| Pause transfer | Available. Prevents future claims; pausing an already-running `rsync` is best-effort until cooperative process interruption exists. |
| Resume transfer | Available from `paused`; continues from bytes already transferred. |
| Cancel transfer | Already available for queued, running, failed, or retryable transfer records. |
| Retry transfer | Already available from failed, retryable, or cancelled transfer records. |
| Verify transfer | Available. Re-runs local destination manifest/checksum verification and marks the transfer `completed` with checksum `verified` on success. |
| Delete transfer record | Available for inactive transfer records; it removes the transfer row only, not artifacts. |

Transfer UI must treat partial progress as normal. It must expose source, destination, bytes total, bytes done, checksum status, current error, and last update time. The current backend does not store retry count separately, so show state history through transfer timestamps and status until transfer events exist. Cancel is durable state, but remote `rsync` cancellation is not guaranteed to kill an already-running process until the worker observes the terminal state on the next update. Running transfers older than `BUBA_RESEARCH_TRANSFER_STALE_MS` are automatically marked `retryable`; the UI should present that as restart recovery rather than data loss.

### Jobs

Required operations:

| Operation | Need |
| --- | --- |
| Create job | Already available. |
| Read/list job | Already available. |
| Update queued job | Available for priority, params, and artifact corrections while no step has started. |
| Cancel job | Already available. |
| Pause job | Available as durable queue control. Running child-process interruption is still best-effort until worker cooperation exists. |
| Resume/continue job | Available. Continues paused, cancelled, blocked, failed, or retryable work without rerunning completed steps when safe. |
| Retry job | Already available for failed/blocked/retryable/cancelled. |
| Clone job | Available. Creates a new queued job from prior type, artifact, params, and optional overrides, with provenance recorded as a job event. |
| Delete job record | Available for inactive jobs that have no reports. This removes job metadata, steps, and events only. |
| Recompute report | Available. Regenerates JSON/CSV from persisted job, step, event, and report metadata without rerunning compute commands. |
| Add operator note | Already possible through append event, but should be framed as notes/audit. |

Available API endpoints:

```text
PATCH /api/research/jobs/{id}
POST /api/research/jobs/{id}/pause
POST /api/research/jobs/{id}/resume
POST /api/research/jobs/{id}/continue
POST /api/research/jobs/{id}/clone
POST /api/research/jobs/{id}/report/regenerate
DELETE /api/research/jobs/{id}
```

Job detail must expose step state, input/output JSON, error, attempts, lease owner, lease expiry, created/started/completed timestamps, and events.

Current backend cancellation is durable state cancellation. It prevents future leases and marks unfinished steps cancelled, but it should not be presented as guaranteed immediate termination of a command already running inside a worker until the worker has cooperative cancellation or child-process kill support.

### Steps

Required operations:

| Operation | Need |
| --- | --- |
| Retry step | Available for failed, blocked, retryable, cancelled, or paused steps if dependencies allow it. |
| Skip step | Not implemented; there are no explicitly safe optional step templates yet. |
| Cancel running step | Available as durable operator intent. Worker child-process interruption is still best-effort until worker cooperation exists. |
| Clear stale lease | Available for expired active leases only. |
| Mark external prerequisite resolved | Available for blocked steps after an operator action. |

Available API endpoints:

```text
POST /api/research/jobs/{job_id}/steps/{step_id}/retry
POST /api/research/jobs/{job_id}/steps/{step_id}/cancel
POST /api/research/jobs/{job_id}/steps/{step_id}/clear-lease
POST /api/research/jobs/{job_id}/steps/{step_id}/resolve-blocker
```

Missing API examples:

```text
POST /api/research/jobs/{job_id}/steps/{step_id}/skip
```

The UI must not expose unsafe step operations without backend support. Disabled controls are acceptable only when they explain the missing prerequisite.

### Reports

Required operations:

| Operation | Need |
| --- | --- |
| Read/list report metadata | Already available. |
| Open report JSON | Available. |
| Open/download report CSV | Available. |
| Rename/title report | Available. |
| Add notes/tags | Missing backend support. |
| Archive report | Available. |
| Delete report | Available for metadata only or metadata plus files, with explicit distinction. |
| Link report to source job/artifact | Already stored but should be visible and navigable. |

Available API endpoints:

```text
PATCH /api/research/reports/{id}
POST /api/research/reports/{id}/archive
POST /api/research/reports/{id}/restore
DELETE /api/research/reports/{id}
GET /api/research/reports/{id}/json
GET /api/research/reports/{id}/csv
```

## Functional Workflows To Support

### Export A Live/Readonly Run

Minimum required flow:

1. Select source machine/run context.
2. Provide source DB path and optional log paths.
3. Choose run mode: `paper`, `live_readonly`, or `live_trading`.
4. Choose source state: `stopped` or `running_readonly`.
5. Set interval metadata when known.
6. Run dry-run by default.
7. Show safety status and reasons from backend.
8. Block real export when safety status is blocked.
9. Require explicit confirmation for `dry_run=false`.
10. Create the export job.
11. Follow steps until complete, blocked, failed, cancelled, or retryable.
12. Verify artifact.
13. Open created artifact and report.

The UI must preserve the distinction between dry-run and real export. Dry-run must not be presented as having created an artifact.

### Create Current-Params Backtest

Minimum required flow:

1. Select an available artifact.
2. Confirm replay and backtest quality metadata when present.
3. Choose interval from artifact metadata or override explicitly.
4. Set balance and optional parameter overrides.
5. Create `current_params` job.
6. Follow validation, preparation, backtest, and report steps.
7. Support cancel, retry, resume/continue, and clone from completed/failed jobs.
8. Open report and source artifact from job detail.

### Create Sweep

Minimum required flow:

1. Select an available artifact.
2. Require at least one sweep dimension.
3. Validate sweep dimensions before submit when possible.
4. Set interval, balance, and optional base parameter overrides.
5. Create `sweep` job.
6. Follow validation, preparation, sweep, and report steps.
7. Support cancel, retry, resume/continue, and clone.
8. Preserve sweep params in history so the run can be repeated.

### Recover From Failure

Minimum required flow:

1. Identify whether failure is transfer, artifact verification, validation, prepare, backtest, sweep, report, or archive.
2. Show the current failing step and backend error.
3. Offer only valid actions for that state.
4. Allow operator note before or after action.
5. Retry/resume without rerunning already completed steps unless the operator explicitly chooses a full rerun or clone.
6. Preserve failure history in events.

### Archive Scratch Data

Minimum required flow:

1. Show which scratch DBs are eligible for removal.
2. Confirm reports and manifest exist before deletion.
3. Show what will be deleted and what will be preserved.
4. Execute archive operation.
5. Show archive result in report/job history.
6. Keep report metadata and artifact provenance searchable.

Backend guarantees:

* `archive_scratch=true` runs only during `write_report`.
* The worker writes report JSON, report CSV, and report metadata before deleting scratch DB files.
* Archive cleanup only targets prepared/backtest SQLite DB families under the job root.
* Artifact manifests, source DBs, transferred artifact roots, report JSON, and report CSV are not archive targets.
* If archive validation or deletion fails, the report stays available, report JSON includes `archive_error`, and the job is blocked for operator recovery.

## Entity Operation Matrix

The frontend implementation should track this matrix. A row is not done until the operation is either implemented or explicitly blocked by a backend ticket.

| Entity | Create | Read/list | Update | Cancel/stop | Pause | Resume/continue | Retry | Archive | Delete | Clone |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Machine | Required | Available | Required | Disable required | Optional | Enable required | Health retry optional | Not applicable | Required with guards | Not applicable |
| Artifact | Required | Available | Required | Not applicable | Not applicable | Restore required | Verify retry required | Required | Required with guards | Optional |
| Transfer | Available | Available | Optional | Available | Required | Required | Available | Optional | Required | Optional |
| Job | Available | Available | Required for queued | Available | Required | Required | Available | Optional | Required soft-delete | Required |
| Step | Worker-created | Available via job | Limited | Required | Optional | Required | Required | Not applicable | Not applicable | Not applicable |
| Report | Worker-created | Metadata available | Required | Not applicable | Not applicable | Restore required | Regenerate required | Required | Required with guards | Optional |

## Permissions

Observer users:

* May list and read machines, artifacts, transfers, jobs, events, and reports.
* May not create, update, cancel, retry, delete, archive, restore, or append operator notes.

Admin users:

* May perform all supported operations.
* Destructive operations require explicit confirmation.
* Dangerous operations must include a clear backend reason in the event/audit trail.

## State Rules

Implement controls from state, not from optimistic guesses.

| State | Allowed operator actions |
| --- | --- |
| `queued` | update queued params, cancel, pause/hold when API exists |
| `leased` | cancel intent, wait for lease expiry, clear stale lease when API exists |
| `running` | cancel intent, watch events/logs |
| `completed` | open report/artifact, clone, archive report/artifact, delete with guards |
| `blocked` | resolve blocker, retry/resume, cancel, add note |
| `retryable` | retry/resume, cancel, add note |
| `failed` | retry, clone, cancel/archive, add note |
| `cancelled` | resume/continue when safe, clone, delete/archive, add note |

The UI must never imply that cancelling a running external process is instantaneous unless the backend confirms it. It should represent cancel as requested, then terminal when persisted.

## Data Contracts To Model In TypeScript

The UI needs typed client models for:

* `ResearchMachine`
* `ResearchArtifact`
* `ArtifactTransfer`
* `ResearchJob`
* `ResearchJobStep`
* `ResearchJobEvent`
* `ResearchReport`
* `CreateResearchJobRequest`
* `ResearchJobDetailResponse`
* operation responses and errors

Use the Rust fields in `dashboard/server/src/db.rs` and endpoint shapes in `dashboard/server/src/api/research.rs` as the source of truth until OpenAPI or generated types exist.

## Required History And Provenance

Every object detail must make these relationships accessible in both directions:

* Machine to artifacts created on that machine.
* Artifact to source machine, manifest, transfers, jobs, and reports.
* Transfer to source artifact, source machine, destination machine, checksum result, and job if applicable.
* Job to artifact, steps, events, output report, params, requester, created/completed/cancelled timestamps.
* Step to command output, error, attempts, lease owner, and timestamps.
* Report to job, artifact, JSON path, CSV path, archive state, and notes/tags when added.

## Edge Cases The UI Must Handle

* Empty system with only seeded machines.
* `research` machine status can be `not_configured`, configured but idle, unreachable, or running a worker.
* Export dry-run completes without creating an artifact.
* Export is blocked for `live_trading`.
* Existing WAL bytes are reported but not copied.
* Artifact exists but manifest file is missing or checksum fails.
* Backtest job is blocked because validation failed.
* Worker dies mid-step and lease expires.
* Cancelled job has completed earlier steps and queued/cancelled later steps.
* Report metadata exists but report file is missing.
* Report file is missing but the job/report metadata can be regenerated.
* Transfer is half complete and retryable.
* User is observer and every mutation must be unavailable.

## Acceptance Tests For The UI Work

At minimum, add frontend tests for:

* Observer can open research pages and cannot mutate.
* Admin can create export dry-run job.
* Admin can create current-params job from artifact.
* Admin can create sweep job with at least one sweep dimension.
* Job detail shows steps and events.
* Cancel action calls cancel endpoint and refreshes job detail.
* Retry/resume action calls the correct endpoint and preserves completed steps in displayed history.
* Blocked job shows backend error and valid next actions.
* Dry-run export is not displayed as an available artifact.
* `live_trading` export block is visible and cannot be bypassed.
* Artifact verification failure is visible.
* Transfer partial progress can be rendered.
* Report list links back to job and artifact.
* Missing backend operation appears as unavailable, not silently omitted.

## Backend Acceptance Before Calling The Research UI Complete

Do not call the Research UI complete until either these exist or the product owner explicitly cuts scope:

* Notes/tags schema support for artifacts, jobs, and reports if required for v1.
* Frontend tests consume the seeded fixture states instead of only testing empty or happy-path pages.

## Suggested Implementation Order

1. Add TypeScript research models and API client calls for currently available endpoints.
2. Add read-only research pages that expose machines, artifacts, transfers, jobs, job detail, events, and reports.
3. Add admin machine create, update, disable, enable, health, and guarded delete controls.
4. Add admin job creation for existing job types.
5. Add admin cancel, pause, resume/continue, retry, update, clone, and guarded delete using current endpoints.
6. Add report JSON/CSV reads and archive/restore/delete controls.
7. Add explicit UI placeholders or disabled actions for every remaining missing lifecycle operation in the matrix.
8. Work with backend to fill remaining CRUD/control endpoints.
9. Replace placeholders with live actions as endpoints land.
10. Add fixtures/tests for all edge states before visual polish.

## What This File Does Not Specify

This file does not prescribe visual design, layout, chart style, component composition, spacing, color, or exact copy. The frontend colleague owns those decisions.

The only UI constraint from this document is functional: lifecycle operations and state recovery must be first-class and testable.
