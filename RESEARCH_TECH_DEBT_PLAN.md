# Research Subsystem Technical-Debt Remediation Plan

## Context

The Research subsystem is the dashboard-side experiment control plane of buba-paint. It lets operators define research jobs (export, backtest, sweep), lease and run them through a worker that shells out to `buba-paint`, capture artifacts, transfer them between machines, and turn results into reports. It spans:

* `dashboard/server/src/research_*.rs`, `dashboard/server/src/api/research.rs`, `dashboard/server/src/api/research_workers.rs`, `dashboard/server/src/bin/buba-research-worker.rs`, and the `research_*` tables in `dashboard/server/src/db.rs`.
* `dashboard/client/src/pages/research-*.tsx`, `dashboard/client/src/components/research/*`, `dashboard/client/src/hooks/use-research-*`, `dashboard/client/src/lib/research-*`.
* `docker-compose.research.yml`, `scripts/deploy-machine.py`, `scripts/research-maintenance.py`, `scripts/seed-research-fixtures.py`, and `RESEARCH_ORCHESTRATION.md`.

The worker runs in two backends: a local SQLite `DashboardDb` and a remote `ResearchControllerClient` that leases and persists all work over an authenticated HTTP protocol (Phase 15, the `/api/research/workers/*` surface, gated by a shared `BUBA_RESEARCH_WORKER_TOKEN`).

### Goal

Eliminate accumulated technical debt in the Research subsystem without breaking the running system and without changing any trading behavior. Every fix in this plan is confined to dashboard server/client code, research worker plumbing, tests, docs, scripts, and Compose/deploy config.

### Hard guardrails (apply to every task)

* Do NOT change trading-strategy logic, decision math, or signal/backtest/replay numerics. Strategy code under `bots/paint/src/strategies`, `bots/paint/src/decision`, and the core backtest/replay math is off-limits. None of the verified findings touch it; if any task ever appears to, stop and flag it instead of editing.
* Real-money trading is deferred. `paper` and Docker/Caddy `live_readonly` are the only active postures.
* Keep paint and sidecar stopped/undisturbed. Research changes that deploy must use `docker compose ... --no-deps` on dashboard/agent/worker services only, per the memory rule and `docs/deployment-and-ops.md`.
* Preserve every currently-green gate. Each phase ends with an exact verification checkpoint that must pass before proceeding.
* Behavior-adjacent control flow (report-regeneration and terminal-status predicates) must keep byte-identical matched value sets when refactored. These are flagged in-line.

## Definition of done

* Every task in Phases 0 through 5 is either implemented and verified, or explicitly marked flag-only and left unchanged with a recorded rationale.
* All checkpoints pass: `cargo test -p buba-dashboard`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --all --check`, `make lint`, `make comment-audit`, `make docs-audit`, `make hot-path-audit`, `cd dashboard/client && npm test && npm run lint && npm run build`, `make coverage-gate`, and `make test-e2e`.
* No change to any file under `bots/paint/src/strategies`, `bots/paint/src/decision`, or the core backtest/replay numerics. `git diff --name-only` over those paths is empty.
* A research e2e flow exists and passes, and a research-scoped coverage floor is wired into `scripts/check_coverage.py`.
* The durable docs under `docs/` describe the Phase 15 worker-controller protocol, and the stale heartbeat-only framing is corrected in the worker binary rustdoc, `docs/deployment-and-ops.md`, and `research_worker.rs`.
* Deployed-behavior phases have been published and browser-accepted per `RESEARCH_ORCHESTRATION.md`, with `RESEARCH_ORCHESTRATION.md` updated to reflect current state.

## How to run this plan autonomously

Work one phase at a time, and within a phase one task at a time. The per-phase loop:

1. Read the cited files at the cited line refs before editing. Line numbers are from the review snapshot and may have drifted; re-locate by symbol/content, not by absolute line.
2. Implement the task. Add a regression test for every behavioral fix (most tasks below already name one).
3. Run that phase's Verification checkpoint commands. Only proceed if every command is green.
4. Commit with a single imperative-mood line in the repo style (for example `Reap supervised research child on poll error`). No co-author trailer, no AI attribution, no mention of Claude/Anthropic/assistant in any git or GitHub artifact.
5. For phases that change deployed behavior, after local gates are green, publish images and run the browser-acceptance steps from `RESEARCH_ORCHESTRATION.md`, then record the result.

Rules while looping:

* If a checkpoint fails, fix the code (TDD: fix the implementation unless the expected value is demonstrably wrong). Do not weaken a test or a gate to make it pass.
* If a task turns out to require touching strategy/decision/backtest numerics, stop, leave it unchanged, and record it as flag-only.
* Do not create scratch DBs, logs, or evidence bundles in the repo root. Use `/tmp` or an ignored data path. Do not edit `runs/`.
* Prefer fixing one finding per commit so a regression can be bisected.

### Dedup note

Several findings overlap and are consolidated so they are fixed once:

* The supervised-child leak (`supervised-child-leak-on-poll-error`) and the lease-steal double-run (`lease-steal-double-run-during-long-command`) share the same root cause (no `kill_on_drop`, refresh/cancel errors abandon the child). They are fixed together in Phase 0, Task 1.
* The auth-bypass prefix findings (`auth-bypass-prefix-match-fragile`, `worker-prefix-auth-bypass-by-string`) are the same issue from two dimensions; one Phase 0 task covers both.
* The worker artifact id validation findings (`worker-token-no-per-actor-authz` point 2, `worker-upsert-artifact-id-unvalidated`) overlap and are fixed in one Phase 2 task; the per-actor authz design observation from `worker-token-no-per-actor-authz` is handled as a scoped Phase 0 hardening task.
* The remote error-mapping findings (`remote-error-status-collapses-to-internal`, `send-treats-non-json-2xx-as-decode-internal`, `poll-decode-error-crashes-worker-loop`, `send-required-204-misreported-as-no-content`) are one protocol-parity cluster, fixed together in Phase 1, then locked by the parity tests in `no-error-mapping-parity-tests` and `worker-protocol-failure-paths-untested`.
* The stringly-typed status/job-type and magic-string findings (`artifact-status-not-enum-validated`, `job-type-and-status-magic-strings`, `two-similar-regen-predicates-naming`) are sequenced so validation lands in Phase 2 and the const/enum cleanup lands in Phase 5.
* The docs-staleness cluster (`durable-docs-omit-worker-controller-protocol`, `worker-bin-rustdoc-heartbeat-only-framing`, `deployment-doc-stale-heartbeat-claim`, `research-worker-module-doc-stale-future-tense`, `sysarch-codemap-omits-phase15-files`, `no-durable-research-architecture-chapter`, and the `RESEARCH_ORCHESTRATION.md` hygiene items) is one Phase 4 effort.
* The duplication cluster (`path-to-string-duplicated-5x`, `current-epoch-ms-and-optional-env-duplicated`, `research-artifact-for-job-duplicated`, `research-artifact-record-handbuilt-6x`, `frontend-report-metric-formatters-duplicated`, `research-work-backend-triple-delegation`) and the oversized-file decomposition items are Phase 5.

Severity below uses the adjudicated `adjusted_severity` from the verified set, not the original proposer severity.

## Phase 0: Critical concurrency, correctness, and security hardening

Objective: stop the highest-impact failure modes (orphaned/duplicated `buba-paint` children, intermittent SQLITE_BUSY data loss, destructive side effects on cancelled jobs) and the security exposures that have real blast radius, with the lowest-risk fixes first.

Tasks:

1. Reap the supervised `buba-paint` child on every early return, and never let it outlive its lease owner. Consolidates `supervised-child-leak-on-poll-error` (high) and `lease-steal-double-run-during-long-command` (low).
   * Files: `dashboard/server/src/research_pipeline.rs:433-486` (the spawn at 441 and the supervision loop at 459-474), `terminate_cancelled_child` (the existing TERM/KILL path used by the cancellation branch at line 467).
   * Change: set `.kill_on_drop(true)` on the tokio `Command`, and wrap the supervision loop so that on any early return (try_wait error at 462, `is_cancelled().await?` error at 465, `refresh_lease().await?` error at 470) the code terminates the child via the existing `terminate_cancelled_child` path before returning. Treat `is_cancelled`/`refresh_lease` errors as non-fatal to the child: log a warning and keep supervising rather than abandoning the running process.
   * Why: a transient DB-busy or controller hiccup currently orphans a CPU-heavy child that nothing reaps, and the lease design then lets a restarted/other worker spawn a second `buba-paint` against the same scratch DB paths, corrupting prepared/backtest/sweep outputs.
   * Fix risk: low. Strategy-safety: this manages only the child OS-process lifecycle; the `buba-paint` backtest output is unchanged. No strategy/decision/backtest code is touched.
   * Regression test: add a `research_pipeline` test that a `refresh_lease`/`is_cancelled` error during supervision terminates the child and does not propagate a worker-killing error, and a test asserting `kill_on_drop` behavior via a fast-exiting fake command.

2. Set a SQLite `busy_timeout` on the shared connection and classify retryable lock errors. Addresses `no-sqlite-busy-timeout-multiprocess` (medium).
   * Files: `dashboard/server/src/db.rs:692-716` (`DashboardDb::new`), `dashboard/server/src/error.rs:7,35`.
   * Change: immediately after opening the connection and setting `journal_mode=WAL`, set `conn.busy_timeout(Duration::from_secs(5))` (or `pragma_update(None, "busy_timeout", 5000)`), and consider `PRAGMA synchronous=NORMAL` under WAL. Optionally add a bounded retry wrapper around write transactions that treats `SQLITE_BUSY`/`SQLITE_LOCKED` as retryable.
   * Why: `research-dashboard` and `research-worker` open independent connections to the same WAL file (`docker-compose.research.yml`), so an overlapping operator write and worker write can return `SQLITE_BUSY` and surface as a 500 with no retry, dropping job events/progress and interacting with the child-leak above.
   * Fix risk: low. Strategy-safety: DB connection setup only; no trading logic.
   * Regression test: a `db` test asserting `busy_timeout` is set on a fresh `DashboardDb` (query `PRAGMA busy_timeout`).

3. Re-check cancellation before destructive report-step side effects, and defer scratch archival until after durable completion. Addresses `write-report-side-effects-on-cancelled-job` (medium).
   * Files: `dashboard/server/src/research_worker.rs:474-533` (`write_report_step`) and `656-678`/`771-831` (metadata persist and completion routing), `dashboard/server/src/research_pipeline.rs` `archive_scratch_dbs`/`archive_db_family` (the `remove_file` path), `dashboard/server/src/db.rs:2875-2921` (`complete_research_step_at` guard) and `create_or_update_research_report`.
   * Change: at the start of `write_report_step` and again immediately before `archive_scratch_dbs` and `publish_report_documents`, re-check `cancellation.is_cancelled()` / job status, and bail out without deleting scratch or publishing if the job is cancelled. Move the destructive archival so it runs only after the step is durably completed.
   * Why: an operator cancel during report generation currently leaves an undeletable `available` report row and irreversibly deletes the prepared/backtest scratch DBs, so a retry cannot reuse prepared inputs.
   * Fix risk: low. Strategy-safety: worker orchestration only; report contents/numerics unchanged.
   * Regression test: a worker test that cancelling between metadata persist and completion leaves no published report and does not delete scratch DBs.

4. Add an attempt cap so a poison step transitions to failed instead of re-leasing forever. Addresses `unbounded-step-attempts-no-poison-cap` (low).
   * Files: `dashboard/server/src/db.rs:2702-2787` (`lease_next_research_step_at`, increment at 2758, candidate predicate at 2730-2733), `dashboard/server/src/research_worker.rs:271-336`.
   * Change: add a max-attempts constant or config. In `lease_next_research_step_at`, when a candidate step already has `attempts >= max`, transition it to `failed` (and the job to `failed`) with a clear error instead of re-leasing. Keep the existing operator `retry_research_step` to deliberately reset and try again.
   * Why: a step that repeatedly hard-kills the worker before completion is re-leased indefinitely, each re-lease spawning another expensive `buba-paint`, with no terminal convergence and only a manual stale-lease escape hatch.
   * Fix risk: low. Strategy-safety: lease/fail transitions in the DB boundary only.
   * Regression test: a `db` test that a step exceeding the attempt cap is marked failed on the next lease attempt rather than re-leased.

5. Make the worker-token auth exemption structural instead of a string-prefix convention. Consolidates `auth-bypass-prefix-match-fragile` (low) and `worker-prefix-auth-bypass-by-string` (low).
   * Files: `dashboard/server/src/auth.rs:83-91` (the `path.starts_with("/api/research/workers/")` bypass at 87), `dashboard/server/src/main.rs:104-117,148-261` (router assembly, `research_worker_protocol_routes`, the heartbeat route, and the existing `DefaultBodyLimit::max(64*1024*1024)`), `dashboard/server/src/api/research.rs:3329` (`require_worker_token`), `dashboard/server/src/api/research_workers.rs`.
   * Change: build the worker routes (the protocol subtree plus `/api/research/workers/heartbeat`) as a separate `Router` with a dedicated worker-token middleware layer, and mount the JWT `require_auth` layer only on the operator router. Remove the string-prefix bypass once the worker-token layer covers the same routes. Preserve fail-closed behavior, the 64MB `DefaultBodyLimit`, and header handling.
   * Why: today every worker route is protected only because each handler remembers to call `require_worker_token`; a future route under that prefix that forgets the check is fully unauthenticated from the public edge.
   * Fix risk: medium (an auth-layering refactor can regress if done carelessly). Strategy-safety: dashboard auth/route wiring only.
   * Regression test: an HTTP test asserting a worker route rejects a missing/invalid token with 401 even if a handler did not call the helper, and that operator JWT routes still require a valid JWT.

6. Move the heartbeat route under an explicit body limit and bound its telemetry payload. Addresses `heartbeat-no-body-limit-and-worker-token-bypass` (low).
   * Files: `dashboard/server/src/main.rs:148` (heartbeat registration), `dashboard/server/src/api/research.rs:861,500-512` (`worker_heartbeat`, `WorkerHeartbeatRequest`), `dashboard/server/src/db.rs:3520-3545` (`validate_research_machine_heartbeat`).
   * Change: register the heartbeat route inside the worker-token router (or apply an explicit, smaller `DefaultBodyLimit` to it), and clamp/validate `samples.len()` plus the size of `details`/`activity` in the validator before persisting.
   * Why: the heartbeat currently falls back to axum's default body cap and accepts unbounded `samples`, the only accumulating telemetry table; a token-holding caller could bloat it.
   * Fix risk: low. Strategy-safety: telemetry/heartbeat path only.
   * Regression test: a validator unit test rejecting an over-cap `samples` count and oversized `details`/`activity`.

7. Bind worker artifact/report upserts to a safe id and validate the record id. Consolidates `worker-upsert-artifact-id-unvalidated` (low) and the artifact-id point of `worker-token-no-per-actor-authz` (medium).
   * Files: `dashboard/server/src/api/research_workers.rs:257-265` (`upsert_artifact`), `322-333` (`upsert_report`), `33-47` (`require_safe_document_id`), `dashboard/server/src/api/research.rs:3246` (`validate_artifact_id`).
   * Change: call a safe-id check inside `upsert_artifact` and `upsert_report` before persisting. Because the id is later used as a directory name by `write_work_document`, use the same check the document layer uses (`require_safe_document_id`: alnum plus `- _ .`, no leading dot, no `..`), not the looser `validate_artifact_id`, so the upsert and document paths agree.
   * Why: a worker can otherwise persist artifact/report rows whose id is not a safe directory name, yielding rows that can never store documents and diverging from the validated admin paths.
   * Fix risk: low. Strategy-safety: worker-API validation only.
   * Regression test: HTTP tests that `upsert_artifact`/`upsert_report` reject ids like `../escape`, `.hidden`, and `a/b` with 400.

Verification checkpoint (Phase 0):

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
make lint
make comment-audit
make hot-path-audit
```

Phase 0 changes deployed worker/dashboard behavior. After the above are green and before Phase 1, publish the dashboard/worker images and run the browser-acceptance steps from `RESEARCH_ORCHESTRATION.md` (deploy with `docker compose ... --no-deps` for the research dashboard/worker only; do not disturb paint or sidecar), then record the outcome.

## Phase 1: High-impact correctness and protocol parity (remote backend)

Objective: make the remote `ResearchControllerClient` backend behave like the local `DashboardDb` backend for errors and non-JSON responses, and stop a single malformed controller response from killing the worker loop.

Tasks:

1. Map controller HTTP status to the matching `DashboardError` variant and recover the error envelope. Addresses `remote-error-status-collapses-to-internal` (medium) and the 204 framing in `send-required-204-misreported-as-no-content` (low).
   * Files: `dashboard/server/src/research_controller_client.rs:286-330` (`send`, non-success at 309-313; `send_required` at 320-330), `dashboard/server/src/error.rs:31-52`, with parity reference points `dashboard/server/src/db.rs:2313-2336,929-943`.
   * Change: in `send`, map `NOT_FOUND -> NotFound`, `BAD_REQUEST -> BadRequest`, `UNAUTHORIZED -> Unauthorized`, `FORBIDDEN -> Forbidden`, and 5xx/other -> `Internal`. Parse the `{"error": msg}` envelope to recover the bare message so remote and local error text match. Fold the `send_required` 204 case into the status mapping so a 204 on a mutation maps to a meaningful variant rather than a generic "returned no content" Internal.
   * Why: the client currently collapses every non-success status into `Internal`, breaking variant parity with the local backend for the same logical outcome (for example a missing-id cancel is `NotFound` locally but `Internal` remotely).
   * Fix risk: low. Strategy-safety: HTTP client error mapping only.

2. Guard `send` against non-JSON 2xx bodies with a clear, classifiable error. Addresses `send-treats-non-json-2xx-as-decode-internal` (medium).
   * Files: `dashboard/server/src/research_controller_client.rs:302-318`, with context `dashboard/server/src/main.rs:70-80` (SPA fallback), `dashboard/server/src/auth.rs:83-91`.
   * Change: before decoding a 2xx body, check `Content-Type` is `application/json` (or that the body is non-empty JSON). If not, return a descriptive `DashboardError` (for example `NotFound`/`Proxy`) that names the unexpected content type and includes a short body prefix, instead of the opaque "expected value at line 1 column 1" decode error.
   * Why: under version skew the SPA HTML fallback returns 200 text/html for an unknown worker route, which currently becomes a cryptic decode error.
   * Fix risk: low. Strategy-safety: client decoding only.

3. Make per-tick polling failures non-fatal to the worker loop. Addresses `poll-decode-error-crashes-worker-loop` (medium).
   * Files: `dashboard/server/src/bin/buba-research-worker.rs:251-282,416-425`, `dashboard/server/src/research_controller_client.rs:314-317,711-718`, `dashboard/server/src/research_worker.rs:84-89`.
   * Change: treat per-tick poll failures (`machine_work_disabled`, and ideally the lease/claim calls) as non-fatal: log a warning, set `activity.last_loop_error`, sleep `poll_ms`, and continue the loop rather than returning `Err`. Keep `run_once` returning the error for one-shot use. Optionally distinguish genuinely-fatal config errors (auth misconfig) from transient decode/network errors so only config errors stop the worker.
   * Why: a single malformed/transient controller response currently kills the long-running worker (a forced container restart that drops the heartbeat task) instead of a graceful sleep-and-retry; this is the observed deploy-log failure mode and is remote-only.
   * Fix risk: low. Strategy-safety: worker loop control flow only.

4. Reduce cancellation poll cost during long commands. Addresses `cancellation-poll-rereads-all-steps-every-500ms` (low).
   * Files: `dashboard/server/src/research_pipeline.rs:104-117,459-474`, `dashboard/server/src/research_worker.rs:430-438`, `dashboard/server/src/db.rs:2126-2142` (`get_research_job_steps`).
   * Change: add a lightweight `is_job_cancelled(job_id) -> bool` (or a single `get_research_step(step_id)` status read) to the backend trait and use it instead of fetching the full job plus every step row, and lengthen the cancellation poll interval (for example 2 to 5 seconds) since operator cancellation does not need sub-second latency.
   * Why: the supervision loop re-reads the whole job and all step rows every 500ms for the entire lifetime of a multi-minute command, contending on the single connection and widening the SQLITE_BUSY window; on the remote backend it is two HTTP round-trips every 500ms per running step.
   * Fix risk: low. Strategy-safety: cancellation-check frequency/shape only; backtest behavior unchanged.

5. Fix the missing-job cancellation path in `run_command_step`. Addresses `cancel-job-notfound-aborts-cancellation-return` (low).
   * Files: `dashboard/server/src/research_worker.rs:440-455,795-831`, `dashboard/server/src/research_pipeline.rs:106-117`.
   * Change: have `is_cancelled` distinguish "job missing" from "cancelled". In `run_command_step`, only call `cancel_research_job` when the job still exists and is not already cancelled (ignore `NotFound`), and let `complete_or_block_pipeline_step`'s cancellation branch own terminal classification. Drop the redundant re-cancel.
   * Why: a deleted/cancelled job during a supervised command currently produces a `NotFound`/block error path instead of a clean cancelled outcome (reachable only under unsupported DB states, but incoherent).
   * Fix risk: low. Strategy-safety: error handling only.

6. Add stale-transfer ownership safety (or enforce single-worker-per-machine). Addresses `stale-transfer-recovery-can-duplicate-active-copy` (low).
   * Files: `dashboard/server/src/db.rs:1747-1771` (`recover_stale_artifact_transfers`), `dashboard/server/src/research_transfer.rs:119-148,219-303` (copy and `rsync_command_spec`), `dashboard/server/src/bin/buba-research-worker.rs:110` (default `stale_after_ms`).
   * Change: prefer adding a `lease_owner`/heartbeat to running transfers (mirror `research_job_steps`) so recovery only requeues transfers not owned by a live worker; or emit periodic `updated_at` progress during long rsync (parse `--info=progress2`). At minimum, document and enforce single-worker-per-machine and make `stale_after_ms` clearly exceed worst-case single-file transfer time.
   * Why: a long single-file rsync can exceed the 30-minute stale window while genuinely live, and under a multi-worker-per-machine topology recovery can requeue an in-flight copy, allowing two writers on the same destination.
   * Fix risk: medium (touches transfer recovery semantics). Strategy-safety: transfer infrastructure only.
   * Regression test: a `db` test that a running transfer owned by a live worker is not requeued by recovery (once ownership is added), or a documented constraint plus a `stale_after_ms` sanity test if taking the minimal path.

Verification checkpoint (Phase 1):

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
make lint
make comment-audit
make hot-path-audit
```

Phase 1 changes remote worker behavior. After local gates are green, publish images and run the `RESEARCH_ORCHESTRATION.md` browser-acceptance steps for the remote worker path (`--no-deps`, research services only), then record the outcome.

## Phase 2: Validation and API/contract consistency

Objective: close server-side validation gaps and align the inconsistent API/contract surfaces so the dashboard and clients agree, without changing any returned numerics.

Tasks:

1. Validate artifact status against a known enum at the SQL boundary. Addresses `artifact-status-not-enum-validated` (medium).
   * Files: `dashboard/server/src/db.rs:1317` (`upsert_research_artifact`) and `3959` (mirror `validate_research_report_status`), call paths `dashboard/server/src/api/research.rs:1062,1125,906` (import/register/update) and `dashboard/server/src/api/research_workers.rs:257-265` (worker upsert).
   * Change: add `validate_research_artifact_status` in `db.rs` accepting the known set (`available`, `archived`, plus any intermediate states workers legitimately persist) and call it from `upsert_research_artifact` before the INSERT. This validates import, register, worker upsert, and PATCH in one place.
   * Why: artifacts currently accept arbitrary status strings while every sibling entity enforces an enum; a typo like `avaliable` renders as available in the UI but silently fails the transfer guard.
   * Fix risk: low (must include the real intermediate states so workers are not broken). Strategy-safety: research-artifact metadata validation only.
   * Regression test: a `db` test rejecting a bogus artifact status and accepting each valid one.

2. Make `cancel_research_job` reject terminal jobs instead of a silent no-op. Addresses `cancel-job-silent-noop-on-terminal` (low).
   * Files: `dashboard/server/src/db.rs:2313` (`cancel_research_job`), mirroring the `retry_research_job` `Some(status) -> BadRequest` arm just below it.
   * Change: when `updated == 0` and the row exists, return `DashboardError::BadRequest` describing the current terminal status, matching `pause`/`retry`/`delete`.
   * Why: cancelling an already-completed/cancelled job currently returns 200 with an unchanged job, inconsistent with sibling transitions and confusing for scripted callers.
   * Fix risk: low. Strategy-safety: control-plane state machine only.
   * Regression test: a `db` test that cancelling a completed job returns `BadRequest`.

3. Bound queue priority input. Addresses `unbounded-priority-input` (low).
   * Files: `dashboard/server/src/db.rs:2017` (job create) and the template create/update boundary, request shapes at `dashboard/server/src/api/research.rs:529,549,1488`.
   * Change: add a bounded check (clamp or reject) for priorities outside a documented range (for example `-1000..=1000`) in the `db.rs` job/template create+update boundary, alongside existing field validation.
   * Why: priority accepts any `i64` with no server-side bound, allowing absurd magnitudes that make queue ordering confusing (no correctness failure, but inconsistent with otherwise-thorough validation).
   * Fix risk: low. Strategy-safety: queue-management validation only.

4. Validate report JSON read failures as a client-actionable error. Addresses `report-json-parse-500-on-corrupt-file` (low).
   * Files: `dashboard/server/src/api/research.rs:1964-1972` (`get_report_json_file`), `dashboard/server/src/error.rs`.
   * Change: map the `serde_json::from_str` parse failure to a clearer error (the enum has no native 422, so use `BadRequest`/400 or the custom-status variant) with a message like "report JSON file is corrupt" including the report id, instead of a 500.
   * Why: a present-but-malformed report file currently yields an opaque 500 rather than signaling a damaged artifact.
   * Fix risk: low. Strategy-safety: error mapping only.

5. Make the artifact-documents both-None contract symmetric. Addresses `artifact-documents-guard-asymmetry` (low).
   * Files: `dashboard/server/src/api/research_workers.rs:267-319` (`store_artifact_documents`), with reference `dashboard/server/src/research_controller_client.rs:613-638` (client guard) and `dashboard/server/src/research_backend.rs:313-321` (local no-op).
   * Change: hoist the both-None short-circuit into the controller handler (return early before `require_work_root`/artifact lookup/re-upsert) so the endpoint matches the documented no-op; keep the client guard as an optimization.
   * Why: the client and local backend treat both-None as a no-op while the controller handler re-upserts and rewrites `artifact_root`, an asymmetric write path.
   * Fix risk: low. Strategy-safety: worker HTTP handler only.

6. Standardize single-entity response wrapping. Addresses `inconsistent-response-wrapping` (low).
   * Files (server): `dashboard/server/src/api/research.rs:737,902,1222,1897,1386` and siblings; (client) `dashboard/client/src/lib/research-api.ts:52-56,113-117,180-184,432-435`.
   * Change: pick one convention (least churn: drop the `MachineResponse`/`JobTemplateResponse` envelopes and return bare entities like artifacts/transfers/reports), update the two TS client return types to match, and document the chosen convention near the response structs.
   * Why: five entity families currently use two different envelope conventions, friction for every future consumer.
   * Fix risk: low. Strategy-safety: HTTP/serialization and client types only.

7. Collapse the duplicate continue/resume job handlers. Addresses `duplicate-continue-resume-job-endpoints` (low).
   * Files: `dashboard/server/src/api/research.rs:1588,1600` (`resume_job`/`continue_job`), `dashboard/server/src/db.rs:2251` (`resume_research_job`).
   * Change: collapse `continue_job` to delegate to `resume_job` (or point both routes at one handler) so there is one handler body. Keep both route paths if the frontend wording must stay.
   * Why: the two handlers are byte-identical and the DB method already dispatches by status, so the verb split carries no backend semantics.
   * Fix risk: low. Strategy-safety: operator-API plumbing only.

8. Humanize requested-by on the queue cockpit. Addresses `queue-shows-raw-user-ids` (low).
   * Files: `dashboard/server/src/api/research.rs:2034` (queue job groups), `1352` (list humanization), `3046` (detail), `3114` (`humanize_job_audit`).
   * Change: call `humanize_job_audit` over the jobs vector inside `build_research_queue_response` before `build_queue_job_groups` (reuse the per-request HashMap cache). Note `recent_reports` and `disabled_hosts` have no user-id field, so only the job groups need it.
   * Why: the same job shows a username on Jobs list/detail but the raw internal user id in the Research home queue groups.
   * Fix risk: low. Strategy-safety: display layer only.

Verification checkpoint (Phase 2):

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
make lint
make comment-audit
cd dashboard/client && npm run lint && npm test && npm run build
```

If Phase 2 changed any operator-visible response shape or status that the deployed dashboard renders, publish and run the `RESEARCH_ORCHESTRATION.md` acceptance steps (`--no-deps`, research services only) before Phase 3.

## Phase 3: Frontend correctness/UX and test depth

Objective: fix the operator-facing frontend correctness and UX defects, remove dead frontend code, then close the test-coverage gaps including a research e2e flow and a research coverage floor. Frontend behavior fixes come before their tests so the tests assert correct behavior.

Frontend correctness/UX tasks:

1. Make report comparison tolerant of partial failures. Addresses `report-compare-promise-all-all-or-nothing` (medium).
   * Files: `dashboard/client/src/pages/research-report-compare.tsx:33-45,55-61`.
   * Change: use `Promise.allSettled`, keep the fulfilled results, surface a non-fatal warning listing the ids that failed to load, and render the comparison from the successfully loaded subset when at least two loaded.
   * Why: one deleted/archived report among a multi-select currently wipes the whole comparison view, inconsistent with the detail page's graceful handling.
   * Fix risk: low. Strategy-safety: frontend only.

2. Throttle the per-row telemetry poll on the machines list. Addresses `machines-list-n-plus-1-telemetry-polling` (medium).
   * Files: `dashboard/client/src/pages/research-machines.tsx:124-161` (row at 131), `dashboard/client/src/hooks/use-research-machines.ts:35-42`.
   * Change: add an `intervalMs`/`enabled` parameter to `useResearchMachineTelemetry` (mirroring `useResearchMachineHealth`) and have the list pass a slower cadence (15 to 30s) or a batched summary; keep the detail page at 5s. Do not change the telemetry payload shape.
   * Why: N hosts open N independent 5s telemetry queries, each triggering a DB aggregation, multiplying agent/proxy load linearly with host count for data that needs only coarse freshness.
   * Fix risk: low. Strategy-safety: frontend polling only.

3. Gate `QueueWaitBanner` machines poll on `queued`. Addresses `queue-wait-banner-polls-machines-on-every-job-detail` (low).
   * Files: `dashboard/client/src/components/research/queue-wait-banner.tsx:37,44-49`, `dashboard/client/src/pages/research-job-detail.tsx:364`.
   * Change: make the machines query conditional on `queued` (add an `enabled` param to `useResearchMachines` mirroring the telemetry hooks, or gate via a wrapper that mounts the polling child only when `job.status === 'queued'`).
   * Why: non-queued job views drive a perpetual 10s machines poll purely to compute a banner that is never shown.
   * Fix risk: low. Strategy-safety: frontend polling only.

4. Humanize the raw enums on artifact detail. Addresses `artifact-detail-raw-snake-case-enums` (low).
   * Files: `dashboard/client/src/pages/research-artifact-detail.tsx:456,476,477`.
   * Change: wrap with the already-imported helpers: `jobTypeLabel(j.job_type)` and `humanize(j.status)`/`humanize(t.status)`, matching the rest of the pages.
   * Why: the linked lists print raw `current_params`/`running` while every other page shows `Backtest`/`Running`.
   * Fix risk: low. Strategy-safety: presentation only.

5. Fix the register-artifact "required" source affordance. Addresses `register-artifact-required-source-not-enforced` (low).
   * Files: `dashboard/client/src/pages/research-artifacts.tsx:588,620,536-550`.
   * Change: the backend register endpoint treats source machine as optional, so drop the `required` prop (or use `hint="Optional"` to match `ImportArtifactDialog`). Do not add a submit guard, since the server accepts an absent source.
   * Why: the field is marked required but submit ignores it, a false affordance inconsistent with the sibling import dialog.
   * Fix risk: low. Strategy-safety: client metadata field only.

6. Reset the confirm-dialog typed phrase on any close. Addresses `confirm-dialog-phrase-not-reset-on-success-for-staying-pages` (low).
   * Files: `dashboard/client/src/components/research/confirm-dialog.tsx:35-44,90-97`, `dashboard/client/src/pages/research-job-detail.tsx:184-191`.
   * Change: add `useEffect(() => { if (!open) setTyped(''); }, [open])` so success-driven closes (archive-scratch) also reset the destructive-confirm phrase.
   * Why: reopening the archive-scratch confirm shows the phrase already satisfied, weakening the type-to-confirm guard on a re-run.
   * Fix risk: low. Strategy-safety: UX only.

7. Reconcile the empty-state vs rendered-rows mismatch in the retention candidate group. Addresses `retention-candidate-group-renders-ineligible-rows` (low).
   * Files: `dashboard/client/src/pages/research-overview.tsx:751-759,764-795`.
   * Change: render `eligibleRows` (or add an explicit "ineligible" subsection with a heading) so the empty-check and the list agree. Optionally prune `retentionSelection` against currently-eligible ids in an effect (defense-in-depth; the server already re-validates, so this is not a correctness fix).
   * Why: a group with only ineligible rows claims emptiness while a mixed group shows ineligible rows as disabled, an inconsistent presentation.
   * Fix risk: medium (selection-pruning interacts with poll cycles). Strategy-safety: frontend only.

8. Add a label map for compound ranking enums. Addresses `humanize-gaps-on-compound-enums` (low).
   * Files: `dashboard/client/src/pages/research-report-detail.tsx:561,285-289`, `dashboard/client/src/lib/utils.ts:94-98`.
   * Change: introduce a small `rankedByLabel(key)` mapping `pnl -> PnL`, `calibrated_pnl -> Calibrated PnL`, and use it in the sweep title (and the raw metric column headers) instead of bare `humanize`. Leave generic statuses on `humanize`.
   * Why: `humanize` lowercases acronyms, rendering `Calibrated pnl`/`Pnl` on a page that elsewhere prints `Net PnL`.
   * Fix risk: low. Strategy-safety: presentation only.

9. Stabilize or remove the BacktestFields polling effect. Addresses `backtest-fields-unstable-polling-effect` (low).
   * Files: `dashboard/client/src/components/research/job-create-form.tsx:1102-1118,663-667`.
   * Change: wrap the parent `onChange` in `useCallback` and either drop the 250ms polling (rely on the input `onChange`/`onBlur`) or key the effect off only `interval_mode` so it is created once per mode switch. The naive "narrow deps to start/end" fix is insufficient because those change per keystroke.
   * Why: the effect tears down and rebuilds a 250ms interval on every render via an unstable `onChange` dependency; an equality guard prevents a render storm but the timer churn is a real smell.
   * Fix risk: medium (the largest component in the subsystem; rely on its existing behavioral test suite). Strategy-safety: form plumbing only.

10. Simplify the save-as-template mutation guard. Addresses `save-template-mutation-missing-onerror-feedback-path` (low).
    * Files: `dashboard/client/src/pages/research-job-detail.tsx:202-224,643-660`.
    * Change: remove the unreachable in-`mutationFn` eligibility guard (the trigger button already enforces `canSaveTemplate`), or keep it with a short clarifying note; keep the dialog error banner. The missing `onError` is benign because `mutate` routes errors into `isError`.
    * Why: the guard duplicates the button gate and is unreachable through normal flow, misleading maintainers.
    * Fix risk: low. Strategy-safety: frontend only.

11. Remove dead research frontend code. Addresses `dead-code-keyvalue-editor-and-unused-exports` (low) and `research-api-dead-exports` (low).
    * Files: `dashboard/client/src/components/research/key-value-editor.tsx:18-89`, `dashboard/client/src/hooks/use-research-jobs.ts:30-37` (`useResearchJobEvents`), `dashboard/client/src/lib/research-permissions.ts:185-187,424` (`jobTypeTone`, `RESEARCH_ACTIONS`), and the eight unused exports in `dashboard/client/src/lib/research-api.ts:46,58,65,71,77,186,258,307` plus the four request types they alone reference (`CreateMachineRequest`, `UpdateMachineRequest`, `TransferProgressRequest`, `UpdateJobRequest`).
    * Change: first relocate the still-live `KeyValueRow` type (imported by `job-create-form.tsx` and `job-form-values.ts`) into a shared types module, then delete the `KeyValueEditor` component, `useResearchJobEvents` (and the now-orphaned `listResearchJobEvents`), `jobTypeTone`, `RESEARCH_ACTIONS`, and the eight dead `research-api.ts` exports and their orphaned request types. Confirm `tsc`/`npm run build` stays green.
    * Why: roughly 120 LOC of dead UI/hook code plus dead client API surface that obscures the real mutation surface.
    * Fix risk: low (must relocate `KeyValueRow` before deleting its file). Strategy-safety: frontend only.

Test-depth tasks:

12. Add error-mapping and failure-path parity tests for the worker protocol. Addresses `no-error-mapping-parity-tests` (low) and `worker-protocol-failure-paths-untested` (low). Depends on Phase 1 Task 1.
    * Files: `dashboard/server/src/tests/research_workers_api_tests.rs:182-437`, `dashboard/server/src/research_controller_client.rs:285-330`. Reuse the existing `spawn_controller` helper.
    * Change: add tests that drive `ResearchControllerClient` against the in-process controller for: missing-id renew/complete/cancel (assert the same `DashboardError` variant as the corresponding `DashboardDb` call now that mapping is fixed); lease as worker-a then renew/complete as worker-b (assert the controller's not-active error surfaces through the client); renew with `lease_duration_ms=None` (assert the 400 propagates); `get_job`/`get_artifact` on a missing id (assert `Ok(None)` via the 204 path); and a route returning 500 with a body (assert the body text appears in the error).
    * Fix risk: low. Strategy-safety: tests only.

13. Add lease-steal-then-original-completes coverage. Addresses `lease-steal-then-original-completes-untested` (low).
    * Files: `dashboard/server/src/tests/db_tests.rs:1266-1302,1480-1525`, guards at `dashboard/server/src/db.rs:2884-2893,2951-2957,3005-3011`.
    * Change: add a case that leases a step as worker-a with a short lease, advances the clock so worker-b reclaims it, then asserts `complete_research_step_at`/`fail_research_step_at`/`block_research_step_at` for worker-a all return `Err` and leave worker-b's lease intact.
    * Fix risk: low. Strategy-safety: tests only.

14. Add document-upload validation and malformed-payload tests. Addresses `no-path-traversal-rejection-test` (low) and `worker-doc-upload-no-validation-or-test` (low). Some overlap with Phase 0 Task 7.
    * Files: `dashboard/server/src/api/research_workers.rs:32-47,267-319,335-354`, `dashboard/server/src/tests/research_workers_api_tests.rs`.
    * Change: add API tests posting ids like `../escape`, `.hidden`, `a/b` to both `/artifacts/{id}/documents` and `/reports/{id}/documents` asserting 400 and that no file is written outside the work root, plus a direct unit table over `require_safe_document_id`. Decide product intent on malformed JSON: either validate `report_json`/`manifest_json` with `serde_json::from_str` before writing (returning `BadRequest`) or document passthrough; add a test pinning the chosen behavior. Note the read side already surfaces a visible error banner, so the client work is a regression test, not net-new error surfacing.
    * Fix risk: low. Strategy-safety: worker-API validation and tests only.

15. Promote shared fixtures to a canonical schema_version 2 report and align the Python/TS fixture shapes. Addresses `fixture-drift-report-schema` (low) and `fixture-drift-telemetry-templates` (low).
    * Files: `scripts/seed-research-fixtures.py:558-590`, `dashboard/client/src/lib/research-fixtures.ts:641-737,122-179`, `dashboard/client/src/lib/research-report-analysis.ts:269-279`, plus `scripts/seed-research-fixtures.py:20-133,257-298` for the missing telemetry/heartbeat and job-template tables.
    * Change: promote at least one shared fixture (both the Python `report_payload` and the TS `fixtureReportJsonPayload` plus the `summary_json` on the available report) to a canonical schema_version 2 document with provenance, metrics, source_comparison, and sweep_summary matching `parseReportPayload`. Keep one explicit legacy fixture. Extend the Python seed schema to create and populate the telemetry/heartbeat and job-template tables mirroring `db.rs`, or document the omission in `RESEARCH_ORCHESTRATION.md`. Add a parity assertion (or a single source-of-truth JSON consumed by both).
    * Fix risk: low. Strategy-safety: fixture/seed code only.

16. Add unit tests for the research hooks and the untested lib/components. Addresses `research-hooks-untested` (low) and `research-lib-and-components-untested` (low).
    * Files (hooks): `dashboard/client/src/hooks/use-research-{jobs,machines,reports,artifacts,transfers,templates}.ts`. Files (lib/components): `dashboard/client/src/lib/research-list-url-state.ts`, `dashboard/client/src/components/research/{step-timeline,event-stream,csv-preview,json-viewer}.tsx`.
    * Change: add a small colocated test per hook (QueryClientProvider plus mocked `research-api`) asserting the query key and any `refetchInterval`/`enabled` gating. Prioritize `research-list-url-state.ts` round-trip parse/serialize including malformed/none/comma input, the `csv-preview` `parseCsv` empty-line edge case, and the `json-viewer` parse-error path. Do not prioritize the thin `research-api.ts` wrappers (they are one-line pass-throughs already covered indirectly).
    * Fix risk: low. Strategy-safety: tests only.

17. Add report-compare edge-case coverage. Addresses `report-compare-shallow-edge-coverage` (low). Depends on Frontend Task 1.
    * Files: `dashboard/client/src/pages/__tests__/research-report-compare.test.tsx`, `dashboard/client/src/lib/research-report-analysis.ts:203-229`, `dashboard/client/src/lib/__tests__/research-report-analysis.test.ts`.
    * Change: add compare-page tests for a single id (StateEmpty), three or more ids, all-null `net_pnl` (assert the "No winner: Net PnL is unavailable" branch of `bestReportLabel`), and partial fetch failure asserting the now-graceful partial render from Task 1. Add a `bestReportLabel` unit test for the `scored.length === 0` branch.
    * Fix risk: low. Strategy-safety: tests only.

18. Add the research e2e flow. Addresses `no-research-e2e-coverage` (medium).
    * Files: new `dashboard/client/e2e/research.spec.ts`, route stubs added to `dashboard/client/e2e/fixtures.ts` (reuse `research-fixtures.ts` shapes), config `dashboard/client/playwright.config.ts`.
    * Change: add research route stubs for `/api/research/*`, then cover: log in, open Research overview, create a `current_params` job via the New job form, open a seeded completed job detail and assert the step timeline plus events, open a report detail and assert the schema_version 2 metrics/equity chart render, and open report compare with two ids asserting the winner label and a provenance-mismatch warning. Gate behind the existing desktop-project skip used in `app.spec.ts`.
    * Fix risk: low (additive). Strategy-safety: tests/fixtures only.

19. Add a research-scoped coverage floor. Addresses `no-research-coverage-floor` (medium).
    * Files: `scripts/check_coverage.py:15-60,91-119`, optionally `dashboard/client/vitest.config.ts`.
    * Change: add a Rust `llvm-cov` invocation scoped to `src/research_.*|src/api/research.*|src/bin/buba-research-worker` with its own minimum (for example 85%), and a frontend research floor parsed from the vitest `coverage-summary.json` by filtering keys under `client/src/.*research`. Wire both into the failures list.
    * Fix risk: low. Strategy-safety: CI tooling only.

Verification checkpoint (Phase 3):

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
make lint
make comment-audit
cd dashboard/client && npm run lint && npm test && npm run build
make test-e2e
make coverage-gate
```

The frontend correctness/UX changes are dashboard-only. After local gates are green, ship them with `docker compose build dashboard-client`-equivalent and `docker compose up -d --no-deps` for the dashboard only (per the auto-deploy-small-dashboard-fixes rule), then spot-check in the browser. The coverage-floor and e2e tasks need no deploy.

## Phase 4: Documentation and comment quality

Objective: bring the durable docs and core-module rustdoc in line with the shipped Phase 15 worker-controller protocol, and de-rot `RESEARCH_ORCHESTRATION.md`. Docs-only; no behavior change.

Tasks:

1. Add a durable Research control-plane section to `docs/`. Addresses `durable-docs-omit-worker-controller-protocol` (medium) and `no-durable-research-architecture-chapter` (low).
   * Files: `docs/system-architecture.md` (add the section, correct lines 116 and 154), or a new `docs/research-orchestration.md`; link from `docs/Readme.md:9-17,76-89`.
   * Change: describe the two-backend model (`ResearchWorkBackend` with a local `DashboardDb` impl and a remote `ResearchControllerClient` over HTTP), the `/api/research/workers/*` worker-token endpoints, the `x-buba-research-worker-token` header auth, the job/step lifecycle and state machine, the artifact register/import/verify lifecycle, worker-owned transfers (append copy vs rsync, stale recovery), and report schema v2 with source-vs-replay comparison. Reference `research_workers.rs` and `research_backend.rs` as executable truth. Verify each nontrivial claim against code.
   * Fix risk: low. Strategy-safety: docs only.

2. Correct the worker binary rustdoc. Addresses `worker-bin-rustdoc-heartbeat-only-framing` (medium).
   * Files: `dashboard/server/src/bin/buba-research-worker.rs:1-5,118,126-128`.
   * Change: update the module doc to state the worker runs against a local DB or a remote controller depending on configuration. Reword `controller_url` and `worker_token` help so it is clear that, when set together, they make the worker lease and persist all job/transfer/report/artifact work through the controller and authenticate every worker-token endpoint, not just heartbeat.
   * Fix risk: low. Strategy-safety: docs only.

3. Correct the deployment doc heartbeat framing. Addresses `deployment-doc-stale-heartbeat-claim` (medium).
   * Files: `docs/deployment-and-ops.md:117`.
   * Change: rewrite the paragraph to state central control is the current production model; that `BUBA_RESEARCH_CONTROLLER_URL` plus `BUBA_RESEARCH_WORKER_TOKEN` make the worker lease all work and report telemetry through the controller; and remove the "final ... deployment" future framing. Verify against `research_workers.rs` and `build_backend`.
   * Fix risk: low. Strategy-safety: docs only.

4. Correct the `research_worker.rs` module rustdoc. Addresses `research-worker-module-doc-stale-future-tense` (medium).
   * Files: `dashboard/server/src/research_worker.rs:1-7`.
   * Change: update the module doc to say the worker leases durable job steps through a `ResearchWorkBackend` that is either the local SQLite `DashboardDb` or a remote `ResearchControllerClient` over HTTP, and executes the allowlisted action per step. Remove the "intentionally local ... can be layered" sentence.
   * Fix risk: low. Strategy-safety: docs only.

5. Add the Phase 15 modules to the system-architecture code map. Addresses `sysarch-codemap-omits-phase15-files` (low).
   * Files: `docs/system-architecture.md:136-159`.
   * Change: add concise code-map entries for `dashboard/server/src/research_backend.rs`, `research_controller_client.rs`, and `dashboard/server/src/api/research_workers.rs`, and update the `buba-research-worker.rs` line to mention local-or-remote backend selection.
   * Fix risk: low. Strategy-safety: docs only.

6. De-rot `RESEARCH_ORCHESTRATION.md`. Addresses `research-orchestration-accreted-changelog` (low), `orchestration-internal-contradiction-pending-deploy` (low), and `orchestration-embeds-volatile-runtime-state` (low).
   * Files: `RESEARCH_ORCHESTRATION.md:9-10,127-297,299-409,442-573`.
   * Change: reconcile the "deployed and accepted" status with the stale "Pending deploy and public acceptance" clause (verify actual deploy state, then drop or relocate the resolved finding). Replace embedded volatile runtime state (container status, sample counts, byte sizes, per-artifact checksums, image digests) with pointers to their executable sources: link `ops/research-images.lock.json` and `ops/live-images.lock.json` instead of pasting digests; reference `GET /api/research/machines/:id/telemetry` and `docker compose ps` for live status; keep only stable identity (machine roles, SSH alias, compose file, work roots) in prose. Preserve the live-run backup checksum but label it as a fixed historical value. Move the dated Manual Evaluation Findings narrative and Browser/API/Local verification chronology into `docs/runs.md` history (or a `data/experiments` evidence index).
   * Fix risk: low. Strategy-safety: docs only. Note: the doc currently pastes image digests that already differ from the lock files, so this also removes a live second source of truth.

Verification checkpoint (Phase 4):

```
make docs-audit
make comment-audit
make lint
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
```

No deploy required for Phase 4. Confirm `make docs-audit` enforces ASCII punctuation, no em dashes/double-dash separators, and the rustdoc/comment policies on the edited modules.

## Phase 5: Behavior-preserving code-quality refactors and decomposition

Objective: remove duplication and decompose oversized modules without changing behavior. Deferred to last because these are the broadest-surface edits; each must be byte-identical in behavior and covered by existing tests. Behavior-adjacent control flow (status/regen predicates) keeps identical matched value sets.

Tasks:

1. Extract a shared `path_to_string` helper. Addresses `path-to-string-duplicated-5x` (low).
   * Files: `dashboard/server/src/research_export.rs:654`, `research_pipeline.rs:853`, `research_transfer.rs:659`, `research_reports.rs:1010`, `api/research.rs:3195`.
   * Change: add one `pub(crate) fn path_to_string(path: &Path) -> String` in a shared research util location (new `research_util.rs` or an existing module) and delete the four other copies.
   * Fix risk: low. Strategy-safety: utility refactor only.

2. Consolidate `current_epoch_ms` and the `optional_env` helpers. Addresses `current-epoch-ms-and-optional-env-duplicated` (low).
   * Files: `dashboard/server/src/api/research.rs:2653-2658`, `research_reports.rs:1200-1206,1003-1008`, `main.rs:534-540`, `bin/buba-research-worker.rs:608`.
   * Change: hoist one `current_epoch_ms` into the shared util and delete the copy. Pick one canonical trimming behavior for `optional_env` (trim-on-read is the safer choice and the two copies read disjoint env vars, so unifying is safe), place it in one helper, and have the worker's `optional_value` reuse the same normalizer.
   * Fix risk: low. Strategy-safety: utility refactor only.

3. Deduplicate `research_artifact_for_job`. Addresses `research-artifact-for-job-duplicated` (low).
   * Files: `dashboard/server/src/api/research.rs:2897-2916`, `research_worker.rs:834-850`.
   * Change: extract one `async fn research_artifact_for_job_id(backend: &impl ResearchWorkBackend, artifact_id: Option<&str>) -> Result<Option<ResearchArtifact>, DashboardError>` in a shared module; have both callers pass `job.artifact_id.as_deref()` / `lease.job.artifact_id.as_deref()` (the API caller passes `&*state.db`).
   * Fix risk: low. Strategy-safety: refactor only.

4. Add `ResearchArtifact::to_record`. Addresses `research-artifact-record-handbuilt-6x` (medium).
   * Files: `dashboard/server/src/db.rs` (add the method mirroring `WorkerArtifactUpsertRequest::as_record`), call sites `api/research.rs:938-955,1085,1139`, `api/research_workers.rs:299-316`, `research_transfer.rs:351`, `research_worker.rs:602`.
   * Change: add `impl ResearchArtifact { pub fn to_record(&self) -> ResearchArtifactRecord<'_> { .. } }`, then at the read-modify-write sites (`update_artifact`, `store_artifact_documents`) use `let mut rec = current.to_record(); rec.field = override; ...; upsert_research_artifact(&rec)`. The manifest-built sites benefit less but can stay as-is.
   * Why: a forgotten field at a read-modify-write site silently drops persisted metadata on a future schema change.
   * Fix risk: low. Strategy-safety: persistence plumbing only.
   * Regression test: a `db` test that `to_record` round-trips all 15 fields.

5. Consolidate the duplicated frontend report-metric formatters. Addresses `frontend-report-metric-formatters-duplicated` (low).
   * Files: `dashboard/client/src/pages/research-report-compare.tsx:201-211`, `research-report-detail.tsx:869-885`, `research-reports.tsx:477-487`; target `dashboard/client/src/lib/research-report-analysis.ts`.
   * Change: move `formatMetricUsd`, `formatPercent`, `formatInteger` (and the detail-only nullable `formatUsd`) into `research-report-analysis.ts` (which owns `ReportMetrics`) and import them in all three pages.
   * Fix risk: low. Strategy-safety: display refactor only.

6. Collapse the `ResearchWorkBackend` triple delegation. Addresses `research-work-backend-triple-delegation` (medium).
   * Files: `dashboard/server/src/research_backend.rs:18-377`, `dashboard/server/src/research_controller_client.rs:350-719,739-1102`.
   * Change: prefer option (b) from the finding because the trait uses RPITIT (`impl Future`) and is not `dyn`-compatible without boxing: generate the `WorkerBackend` enum delegation with a small declarative macro that lists each method signature once, removing the hand-written match arms. Keep `ResearchControllerClient`'s real HTTP impl unchanged (it is not zero-logic).
   * Why: a signature change currently requires synchronized edits in four places; a missed arm is a compile error at best, a silent local/remote gap at worst.
   * Fix risk: medium. Strategy-safety: backend plumbing only; runtime behavior unchanged.

7. Introduce shared constants/enum for job types and lifecycle statuses. Addresses `job-type-and-status-magic-strings` (low). FLAGGED behavior-adjacent: keep matched value sets byte-identical.
   * Files: matchers at `dashboard/server/src/api/research.rs:1824-1826,2469-2484,2758-2764`, `research_pipeline.rs:277`; TS reference `dashboard/client/src/lib/research-types.ts:41`.
   * Change: introduce `const` string constants (or a non-public enum with `as_str`/`from_str`) for job types and statuses in `db.rs` or a `research_constants` module, and reference them from the matchers. The regen/terminal-status matchers gate report regeneration and queue transitions, so this is behavior-adjacent: preserve the exact matched values and rely on the existing tests. Do not change any value set.
   * Fix risk: medium. Strategy-safety: this is dashboard control-plane control flow, not trading strategy. Keep value sets identical and verify with the existing queue/regeneration tests; if a value set would change, stop and flag.

8. Rename the two report-regeneration predicates for axis clarity. Addresses `two-similar-regen-predicates-naming` (low).
   * Files: `dashboard/server/src/api/research.rs:1824-1826,2758-2764`.
   * Change: rename to `job_type_supports_report_regeneration` and `report_status_allows_regeneration` and co-locate them. No value-set change.
   * Fix risk: low. Strategy-safety: rename only.

9. Decompose the oversized modules. Addresses `api-research-rs-oversized-file` (low), `job-create-form-oversized-needs-decomposition` (low), and `research-overview-oversized-needs-decomposition` (low). Do these last.
   * Files: `dashboard/server/src/api/research.rs:1-3383`; `dashboard/client/src/components/research/job-create-form.tsx:1-1601`; `dashboard/client/src/pages/research-overview.tsx:1-1001`.
   * Change (server): split `api/research.rs` into focused submodules under an `api/research/` directory (`dto.rs`, `machines.rs`, `artifacts.rs`, `transfers.rs`, `jobs.rs`, `reports.rs`, `queue.rs`, `retention.rs`, `report_files.rs`) without moving logic; re-export handlers so the router is unchanged.
   * Change (client, job-create-form): extract pure helpers (`buildExportParams`, `buildBacktestParams`, `effectiveInterval`, `parseAdditionalParams`, interval label/source helpers) into a `job-create-params.ts` module with unit tests, and split `ExportFields`/`BacktestFields`/`SweepFields`/`ParameterRowsEditor` into files under `components/research/job-create/`. Keep the public `JobCreateForm` API and submitted payload shape identical; the existing 444-line behavioral suite is the safety net.
   * Change (client, research-overview): move `TemplateDialog`/`parseParams` to `components/research/template-dialog.tsx` and the retention pieces (`RetentionPanel`, `RetentionCandidateGroup`, `RetentionResult`) to `components/research/retention-panel.tsx`, leaving the page as composition.
   * Fix risk: medium (large mechanical moves; high merge surface). Strategy-safety: behavior-preserving moves only; no numerics touched.

Verification checkpoint (Phase 5):

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p buba-dashboard
make lint
make comment-audit
make docs-audit
make hot-path-audit
cd dashboard/client && npm run lint && npm test && npm run build
make coverage-gate
```

Run the full local gate set after the decomposition tasks since they touch many files. These refactors are behavior-preserving and need no deploy of their own; if Phase 5 is shipped, deploy dashboard/agent/worker only with `--no-deps` and confirm the research pages still render.

## Final acceptance (after all phases)

Run the complete gate set and confirm the strategy-untouched invariant:

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo build --release
make test-all
make lint
make comment-audit
make docs-audit
make hot-path-audit
make coverage-gate
cd dashboard/client && npm run lint && npm test && npm run build
make test-e2e
git diff --name-only origin/master -- bots/paint/src/strategies bots/paint/src/decision
```

The final `git diff` over the strategy/decision paths must be empty. Confirm paint and sidecar were never disturbed, that all deployed-behavior phases were published and browser-accepted per `RESEARCH_ORCHESTRATION.md`, and that `RESEARCH_ORCHESTRATION.md` reflects current state with no stale runtime values or internal contradictions.
