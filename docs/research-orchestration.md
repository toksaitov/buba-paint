# Research Orchestration Control Plane

This chapter describes the durable design of the research control plane: how the
dashboard, the worker process, and a controller cooperate to run export,
backtest, and sweep jobs and to move artifacts between machines. It is current
system truth. For deployment wiring see [deployment-and-ops.md](./deployment-and-ops.md);
for the operator UI shape see [system-architecture.md](./system-architecture.md).

Executable truth for this chapter lives in `dashboard/server/src/research_backend.rs`,
`research_controller_client.rs`, `api/research_workers.rs`, `research_worker.rs`,
`research_transfer.rs`, `research_reports.rs`, and the research tables and guards
in `db.rs`. Verify nontrivial claims against those files.

## Two Backend Model

The worker executes the same job and step pipeline regardless of where durable
state lives. The seam is the `ResearchWorkBackend` trait in `research_backend.rs`,
which defines the async operations the worker needs: lease the next step, mark a
step running, complete, fail, or block a step, append job events, check job
cancellation, upsert and verify artifacts, store report and artifact documents,
and claim, progress, and recover transfers.

Two implementations satisfy the trait:

* Local. `DashboardDb` implements the trait by delegating each method to the
  SQLite-backed dashboard database. Document storage is a no-op because the
  worker already wrote the generated files at their recorded paths on the same
  host.
* Remote. `ResearchControllerClient` implements the trait over HTTP, mapping each
  method onto a worker-token endpoint under `/api/research/workers/`. Document
  storage uploads the generated files to the controller so the controller can
  serve them.

A `WorkerBackend` enum wraps either variant and delegates every trait method to
the inner implementation. Its `describe()` reports `local database` or
`remote controller` for operator diagnostics and startup logs.

`build_backend` in `bin/buba-research-worker.rs` selects the backend at startup.
When `BUBA_RESEARCH_CONTROLLER_URL` is set, the worker requires
`BUBA_RESEARCH_WORKER_TOKEN` and builds the remote client; missing the token is a
startup error. When the controller URL is absent, the worker uses the local
database. Controller-based control is the current production model: the deployed
research worker leases and persists all job, step, transfer, report, and artifact
work through its controller, not only heartbeats.

## Worker Token API And Authentication

The remote backend calls worker endpoints under `/api/research/workers/` in
`api/research_workers.rs`. Each request authenticates with the
`x-buba-research-worker-token` header. The server `require_worker_token` check
compares the presented token against the configured token in constant time,
accepts a `Bearer` authorization header as a fallback, and rejects the request as
unauthorized when no worker token is configured.

The endpoints cover the full worker pipeline:

* Steps: claim the next step, renew a lease, mark running, complete, fail, and
  block.
* Jobs: fetch a job, cancel a job, list ordered steps, append timeline events,
  and attach an artifact.
* Artifacts: fetch, upsert, and upload artifact documents.
* Reports: upsert and upload report documents.
* Transfers: claim the next transfer, fetch, post progress, and recover stale
  transfers.
* Machines: fetch a machine row.

Document writes are path-safe. `require_safe_document_id` rejects ids that are
empty, contain characters outside alphanumerics, dash, underscore, and dot, start
with a dot, or contain `..`, and document storage requires
`BUBA_RESEARCH_WORK_ROOT` to be configured. Artifact documents land under
`work_root/artifacts/<id>` and report documents under `work_root/reports/<id>`.

The controller client maps responses to typed results: a 204 becomes an empty
result, 2xx JSON decodes to the response body, and non-2xx maps to a typed error
(not found, bad request, unauthorized, forbidden, or internal) carrying the
server `error` message when present. The client percent-encodes id path segments
and uses a bounded request timeout.

## Job And Step Lifecycle

A job is created `queued`. It moves to `running` when its first step is leased,
can be `paused` and `resumed`, and reaches a terminal `completed`, `failed`, or
`cancelled` state. A failed or blocked job can be retried back to `queued`. The
job status follows from its steps: a blocked step blocks the job, a non-retryable
step failure fails the job, and the job completes when all steps complete.

A step is created `queued` and leased to a worker as `leased`, recording the
lease owner and a lease deadline. The worker marks it `running`, then
`completed`, `failed`, or `blocked`. A failed step becomes `retryable` when
attempts remain, otherwise it fails the job. A blocked step needs an operator to
resolve the blocker before it returns to the queue.

Leases are time-bounded. When a lease deadline passes, the step is eligible to be
re-leased on the next pass as long as its attempt count is under the maximum of
five. A step that reaches the attempt limit is failed as a poison step and fails
its job rather than looping forever.

The worker only runs an allowlisted action per step: plan export, snapshot or
copy runtime, write artifact manifest, verify artifact, validate replay data,
validate backtest input, prepare backtest input, run backtest, run sweep, and
write report.

## Artifact Lifecycle

An artifact is an exported run package with a manifest, checksums, and metadata.
It is created `available`. Archiving sets `archived` and records an archived
timestamp; restoring clears the timestamp and returns it to `available`. Import
registers a local directory, and verify recomputes manifest and checksum state
and reports bytes and files checked. A destination artifact is upserted to
`available` with verified paths and checksum after a transfer completes.

## Worker Owned Transfers

Transfers move an artifact from a source machine to a destination machine. A
transfer is created `queued`, claimed to `running`, and reaches `completed`,
`failed`, or `cancelled`; it can also be `paused`, `resumed`, and `retried`, and
a stale `running` transfer is recovered to `retryable`. Checksum status tracks
`pending`, `verifying`, `verified`, `failed`, or `skipped`, and a completed
transfer requires `verified`.

The worker copies resumably. A local transfer appends to the destination file and
resumes from the existing length; a remote transfer uses rsync with append-verify
and partial flags. Either way `bytes_done` increases monotonically and prior
progress survives a restart.

Exactly one transfer worker runs per destination machine, so a `running` row is
that worker's in-flight copy. Stale recovery therefore uses a safety floor of one
hour: an operator-configured stale age is raised to that floor when nonzero, and
zero or unset disables recovery. This prevents requeuing a long single-file rsync
that the live worker still owns.

## Report Schema Version 2

A completed backtest or sweep step writes a `ResearchReportDocument` with
`schema_version` 2. The full document carries provenance (job metadata, interval,
parameter sets and sweeps, input and output paths, and image refs), metrics (PnL,
fees, balances, trade and signal counts, win rate, and drawdown), an optional
source comparison, an equity curve, a drawdown curve, rejection reasons,
diagnostics, an optional sweep analysis, and step summaries.

The full document is written to the report JSON file, and a compact
`ResearchReportSummary` is stored in the report row. The summary keeps schema
version, generation time, provenance, metrics, source comparison, diagnostics,
step summaries, and a sweep summary with row count and top row, dropping the full
chart arrays.

The source comparison reconciles the live source run against the deterministic
replay. Its status is `matched` when the two align and `mismatch` when net PnL or
final balance differ by more than a cent, or trade or signal counts differ at
all. The document records source metrics, replay metrics, and their deltas so the
dashboard can show where a replay diverged from the run it reproduces.
