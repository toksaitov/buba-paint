# Research Orchestration Tracker

This tracker describes the current Research deployment and readiness state. It
is intentionally concise. Detailed browser QA chronology and screenshots remain
in local evidence folders under `data/experiments/`.

## Current Status

Status: Phase 14 unified public controller is deployed and browser-smoked.
The current cleanup pass has lint, docs, comments, tests, and deploy dry-runs
in good order.

The operator-facing architecture is now:

* `https://buba.toksaitov.com` is the only dashboard URL.
* Monitor pages observe the stopped live-readonly run on `buba-paint`.
* Research pages are available from the same public dashboard.
* `testing` runs the research dashboard and worker as private infrastructure.
* Caddy on `buba-paint` proxies `/api/research*` to `testing` through managed
  tunnel services.
* `paint` and `sidecar` stay stopped until the user explicitly starts a new
  live or paper run.

The current good run is preserved locally and remotely. The live DB checksum is:

`90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e`

Local safety backup:

`data/live-run-backups/live-readonly-20260601-022105/live-readonly-20260601-022105-raw-20260601-183431Z`

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

## Manual Evaluation Findings

These are operator-facing issues found during manual evaluation and their
current status.

* Research > Jobs > New Job interval fallback is confusing. The Start and End
  fields say they fall back to the artifact interval, but it is not obvious that
  leaving both fields blank is the way to trigger that behavior. The page should
  make artifact-derived boundaries explicit before submit, distinguish
  placeholders from real values, and use clearer copy such as "Leave blank to
  use artifact interval." The effective interval preview should remain the
  source of truth and should clearly show whether each boundary came from typed
  input or artifact metadata. Local fix implemented: Start and End now say
  "Leave blank to use artifact interval", and the preview shows typed value
  versus artifact interval source labels. Deployed on 2026-06-02.
* Research job detail does not give enough confidence for long-running command
  steps. During a full-interval current-run backtest,
  `validate_backtest_input` can run for several minutes with an active
  `buba-paint validate-backtest-input` child process, but the UI only shows
  "Running", no output yet, and confusing lease text. The detail page should
  surface command runtime, latest worker heartbeat/activity, and clearer
  "still running" evidence for long-running steps. The worker should also keep
  the active step lease fresh while supervising long commands, or the UI should
  not present an expired lease as recovery evidence while the child process is
  still alive. Local fix implemented: supervised commands now refresh the active
  step lease while they run. Deployed on 2026-06-02. Deeper runtime/activity UI
  remains a follow-up only if operator confidence is still low after the next
  real long-running job.
* Research job recovery diagnosis can show stale command details for the wrong
  step. During the same run, the active step was `validate_backtest_input`, but
  the diagnosis panel showed `validate-replay-data` args, stdout, and status
  from the previous completed step. The diagnosis panel must scope command
  output to the active failed, blocked, or running step before using it as
  operator evidence. Local fix implemented: diagnosis now only uses events for
  the active target step before falling back to that step output. Deployed on
  2026-06-02.
* Research job step lease copy and spacing need cleanup. The `expired` pill is
  too close to the lease sentence, and showing `expired` on long-running steps
  such as `run_backtest` is confusing because those steps are expected to take
  a long time. Long-running command supervision should present worker liveness,
  elapsed runtime, and last activity instead of making a normal long command
  look stale. Local fix implemented: the pill now reads "refresh overdue" with
  explicit spacing, and recovery guidance says to confirm no worker command is
  still running before clearing the lease. Deployed on 2026-06-02.
* Investigate WSL/Docker resource limits on `testing`. The host has 16 physical
  cores plus SMT, but current research work appears to use only about four
  cores. Confirm whether Docker Desktop or WSL is capped for CPU and memory,
  and document or adjust the expected `.wslconfig`/Docker resource settings so
  research backtests can use the intended compute capacity. Verified on
  2026-06-02: WSL reports 32 CPUs and 93 GiB memory, and Docker reports 32 CPUs
  and about 94 GiB memory. Low CPU utilization is therefore more likely
  current-params workload shape or application parallelism than a VM cap.
* Validate report metrics against the real live-run outcome. The first
  full-interval backtest report for
  `live-readonly-20260601-022105-finalized-20260601-185751Z` showed about
  `+45` PnL, while the observed live run result was about `+35`. Investigate
  whether the difference comes from report aggregation, backtest assumptions,
  fees, fills, mark prices, interval boundaries, live accounting, or display
  rounding. Verified: the report `+45.625` matches generated `backtest.db`
  exactly, while the source run DB has `+35.866` over the same interval. The
  replay generated one extra signal and different trade sizes. Local fix
  implemented and deployed: schema v2 current-params reports now include a
  source-run comparison, diagnostics flag, CSV rows, detail warning, and Reports
  list "Source mismatch" flag when replay output differs from source-run
  metrics. The completed report
  `bbc5d6e7-c020-41b1-8c53-58715205c95d` was regenerated on 2026-06-02 and now
  records source net PnL `35.86674472`, replay net PnL `45.62503852`, and
  diagnostic `source_replay_result_mismatch`.
  Local backtester/research update after the 2026-06-02 fidelity pass:
  `prepare-backtest-input` now keeps source `signals`, `signal_metrics`,
  `simulated_trades`, `trade_results`, and `balance_log` rows for the selected
  interval. Sweeps now run one current-params replay baseline when source audit
  rows exist, write raw replay PnL plus source baseline, baseline replay delta,
  calibrated PnL, calibrated final balance, trade/signal deltas, and a
  calibration confidence label to `sweep.csv`, and report generation ranks
  those sweeps by `calibrated_pnl`. This does not make replay an exact
  source-decision auditor: row-level comparison still showed replay filled
  markets `2401941` and `2402338`, while the source run marked those decisions
  missed, and replay did not reproduce late source trades in `2403639` and
  `2403679`. The new behavior makes that replay bias explicit before sweep
  results are used for parameter tuning.
  Latest manual check on 2026-06-02: the finished full-interval report still
  looked reasonable overall, but the operator again noticed replay PnL around
  `+45` versus the live result around `+35`. Treat the comparison warning as a
  required review item before using that report to tune parameters. Browser/API
  evidence:
  `data/experiments/research-manual-qa-playwright/final-chart-and-report-mismatch-check-20260602T010914Z.json`.
* Validate report charts before trusting them for operator decisions. The first
  full-interval report charts looked visually suspicious. Compare chart series
  against the underlying `report.json`, `report.csv`, `balance_log`, and trade
  data, and fix either the analysis output or frontend chart rendering if the
  plotted series are wrong or misleading. Verified root cause: the generated
  equity curve included the initial balance row at timestamp `0`, which made the
  x-axis span from 1970 to the run interval. Local fix implemented: the initial
  balance point is now timestamped at the job start time, so the chart domain
  reflects the actual research interval. The completed report was regenerated
  on 2026-06-02 and its first equity point is now timestamped at
  `1780259776570`, the job start time.
  Latest manual check on 2026-06-02: the regenerated charts still looked odd to
  the operator. Verified that the report JSON is correct: equity ranges from
  `100.0` to `161.08071237`, drawdown ranges from `-31.595666319999992` to
  `0.0`, source net PnL is `35.86674472`, and replay net PnL is
  `45.62503852`. The remaining chart issue was frontend rendering: the y-axis
  was too broad first, then rendered raw-looking decimal labels. Local fix
  implemented and deployed on 2026-06-02: report detail charts now use a padded
  data-domain y-axis and explicit money tick/tooltip formatting. Browser
  evidence:
  `data/experiments/research-manual-qa-playwright/screens/95-current-report-chart-post-money-format.png`.
  Follow-up check on 2026-06-02 verified report detail, report comparison, and
  Research > Machines with zero browser console warnings and zero failed
  requests. Evidence:
  `data/experiments/research-manual-qa-playwright/final-chart-and-report-mismatch-check-20260602T010914Z.json`.
  Post-completion operator review still found the charts visually suspicious,
  even though the report data and money-axis formatting now validate. Keep chart
  readability and replay-versus-source interpretation in the next QA pass.
* Metadata-only delete dialogs were too easy to confirm. Artifact and report
  `Delete record` actions opened a warning dialog, but did not require typing
  the record ID. Local fix implemented and deployed on 2026-06-02: artifact and
  report metadata deletes now require the artifact or report ID before the
  destructive confirm button is enabled. The shared typed-confirm hint now says
  the action cannot be undone from the dashboard, instead of claiming every
  typed-confirm action affects files on disk.
* Login form labels were visible but not programmatically associated with their
  inputs. Browser QA could not target Username or Password by accessible label,
  which is a real keyboard/screen-reader and testability defect. Local fix
  implemented on 2026-06-02: the inputs now have stable IDs and their labels use
  `htmlFor`; the login unit test now uses label-based lookups.
* Reports list filtering was inconsistent with the rest of Research. Operators
  could search Jobs and Artifacts by ID, but Reports only filtered artifact IDs,
  so searching for report ID `bbc5...` did not expose the matching report.
  Local fix implemented on 2026-06-02: Reports now has a general `q=` search
  across report title, report ID, job ID, artifact ID, and job type, while
  legacy `artifact=` URLs remain compatible.
* Artifact checksum loading used a generic `Hide` toggle after loading
  `checksums.sha256`, which made the loaded document less clear in long detail
  pages. Local fix implemented on 2026-06-02: the loaded-state button now says
  `Hide checksums.sha256`, matching the load action and file name.

## Machines

### `buba-paint`

Purpose: public dashboard, Caddy edge, live monitor, and Research API proxy.

Running services:

* `buba-paint-caddy-1`: running.
* `buba-paint-dashboard-1`: running and healthy.
* `buba-paint-agent-1`: running and healthy.

Stopped bot services:

* `buba-paint-paint-1`: exited.
* `buba-paint-sidecar-1`: exited.

Current runtime:

* run: `live-readonly-20260601-022105`
* DB path:
  `/home/ubuntu/buba-paint-live/runtime/live-readonly-20260601-022105/paint.db`
* DB SHA-256:
  `90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e`

Public Research bridge:

* `buba-research-tunnel.service` runs on `testing`.
* `buba-research-proxy.service` runs on `buba-paint`.
* Caddy proxies `/api/research*` to the bridge.
* `https://buba.toksaitov.com/health` returns `{"ok":true}`.

### `testing`

Purpose: research compute and artifact storage host.

Environment:

* SSH alias: `testing`
* WSL distro: `Ubuntu-24.04`
* Remote root: `/home/testing/buba-paint-research`
* Compose file: `docker-compose.research.yml`
* Private dashboard port: `localhost:3002`

Current research services:

* `research-dashboard`: running and healthy.
* `research-worker`: running.
* telemetry through the public dashboard: `stale=false`, worker status `idle`,
  latest response contains 60 samples.

## Artifacts

Research > Artifacts on `https://buba.toksaitov.com` currently shows:

* `live-readonly-20260601-022105-finalized-20260601-185751Z`
  * source: current stopped run
  * status: `available`
  * bytes: `3670922605`
  * stable artifact DB checksum:
    `334180797d06a0f25414bdd7e93782e5f0d812a1f8632d81d4e2419324a70294`
  * preserved source live DB checksum:
    `90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e`
  * interval: `2026-06-01 02:36:16 GMT+6` to
    `2026-06-01 23:37:39 GMT+6`
* `live-readonly-20260514-184119-finalized-20260517-075706Z`
  * source: previous finalized run
  * status: `available`
  * bytes: `6342098944`
  * DB checksum:
    `2f3a778d9955117f7468bec6e459742f7d17417ce8287c7681b61231fba75a81`
  * interval: `2026-05-14 18:34:25 GMT+6` to
    `2026-05-17 13:57:07 GMT+6`

The current artifact manifest and `checksums.sha256` load through the public
dashboard. Manual QA found that the first current-run artifact registration
used a raw SQLite WAL family: the manifest expected the original `paint.db`
size while SQLite later checkpointed the copied DB, so `verify_artifact`
blocked on a byte mismatch. The artifact on `testing` was repaired on
2026-06-02 to a stable single SQLite DB manifest, the controller artifact row
was re-imported, and artifact registration now rejects WAL and SHM sidecars so
future research artifacts must be stable backup DBs.

## Current Image Locks

These locks identify the images currently deployed. Publish fresh images before
the next real deploy if code or image inputs change.

### Research Images

`ops/research-images.lock.json`:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:83e3f83f371e55a48ab9fbb6094986ed9fdb412aa77ed015afcaca35d9107c98`
* research worker:
  `ghcr.io/toksaitov/buba-paint-research-worker@sha256:9ef064d3d1b6f9ea93dc37aee065d2eb93b7784ad58d9445fdbd8011dc61f1ba`

### Stopped-Live Images

`ops/live-images.lock.json`:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:8f594cf69a4af61fa643d61556cca854caecb3250f908002bbda8a8e136b9f51`
* agent:
  `ghcr.io/toksaitov/buba-paint-agent@sha256:cf04bcd42a179d20a44bb550f46972c4953f0e9b199dede9d0d01d1760ca798f`
* paint image published but not running:
  `ghcr.io/toksaitov/buba-paint-bot@sha256:a8f08855fbbedb336d7809573062799939f763e5ed1e2296a15a9cfdc958de83`
* sidecar image published but not running:
  `ghcr.io/toksaitov/buba-paint-sidecar@sha256:e830b3905f28fb61e79f46b3054b188c86b545ec01ed9ad35ccd8199fef7064d`

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

## Latest Browser And API Evidence

The public dashboard was checked through the browser after Phase 14 changes:

* Monitor overview shows `Stopped`, `Readonly`, and the live-readonly bot.
* Research > Artifacts lists both artifacts above.
* The current artifact detail loads manifest and checksum documents.
* Research > Machines shows fresh `testing` telemetry and worker status `Idle`.
* Research > Jobs > New job can select both artifacts.
* Research home and Reports load without Research API errors.
* Report `bbc5d6e7-c020-41b1-8c53-58715205c95d` shows the source mismatch
  warning for the `+45.625` replay result versus the `+35.866` source run
  result.
* The regenerated report chart no longer exposes a 1970 timestamp.
* Report chart y-axis labels now render as operator-scale money values after
  the 2026-06-02 frontend chart fix. Browser evidence:
  `data/experiments/research-manual-qa-playwright/current-report-chart-post-money-format.json`.
  Latest check after the 2026-06-02 redeploy also verifies that the drawdown
  chart ceiling is capped at `$0.00`, with no misleading positive drawdown
  tick:
  `data/experiments/research-manual-qa-playwright/report-replay-source-chart-check-20260602T013758Z.json`.
* Research > Jobs > New job now describes blank Start and End as using the
  artifact interval and no longer shows the internal `artifact fallback` label.
* Full-interval sweep job `6b3b73f3-6d24-44a0-9687-046429e69508` completed
  after the stable artifact repair. Report
  `d1af904f-fe44-4d3d-8a4c-1a274bd6273f` is `available`, renders ranked sweep
  rows, and comparison with the current-params report loads with expected
  compatibility warnings. Browser evidence:
  `data/experiments/research-manual-qa-playwright/sweep-retry-evidence.json`.

Latest stopped-live redeploy on 2026-06-02:

* dashboard:
  `ghcr.io/toksaitov/buba-paint-dashboard@sha256:8f594cf69a4af61fa643d61556cca854caecb3250f908002bbda8a8e136b9f51`
* agent:
  `ghcr.io/toksaitov/buba-paint-agent@sha256:cf04bcd42a179d20a44bb550f46972c4953f0e9b199dede9d0d01d1760ca798f`
* paint:
  `ghcr.io/toksaitov/buba-paint-bot@sha256:a8f08855fbbedb336d7809573062799939f763e5ed1e2296a15a9cfdc958de83`
* sidecar:
  `ghcr.io/toksaitov/buba-paint-sidecar@sha256:e830b3905f28fb61e79f46b3054b188c86b545ec01ed9ad35ccd8199fef7064d`
* `paint` and `sidecar` stayed stopped after deploy.
* Latest finalization browser smoke evidence:
  `data/experiments/research-manual-qa-playwright/finalize-public-smoke-20260602T064237Z/summary.json`.
  It verified deployed login labels, Reports `q=` search, source mismatch
  warning, explicit report JSON and CSV labels, explicit checksum labels,
  read-only Research Machines controls, zero non-navigation failed requests,
  and zero browser console errors.
* Wide browser QA evidence:
  `data/experiments/research-manual-qa-playwright/wide-ui-evidence.json`.
  It covers Research overview, templates, retention, artifact detail, New Job,
  sweep form, job detail dialogs, reports list, comparison route, report
  detail dialogs, transfers empty state, transfer creation dialog, and Machines
  telemetry. The pass recorded 29 checks, 19 screenshots, zero failed API
  responses, and zero browser console errors.
* Template CRUD browser evidence:
  `data/experiments/research-manual-qa-playwright/template-crud-evidence.json`.
  It created a temporary template, edited it, archived it, restored it, deleted
  it, and verified that the temporary template was absent from both the UI and
  API afterward. The pass recorded 9 checks, 9 screenshots, zero failed API
  responses, and zero browser console errors.
* Follow-up public dashboard QA evidence from 2026-06-02:
  `data/experiments/research-manual-qa-playwright/public-research-followup-qa-summary-20260602T0032Z.json`.
  The consolidated pass records 18 verified checks and no unclassified failures.
  It verified transfer filter persistence across sidebar navigation and reload,
  artifact detail return-state and typed delete confirmation, linked artifact
  jobs/reports, completed six-step sweep job detail, clone dialog interval
  guard, report list source-mismatch chip, report detail mismatch warning,
  report chart money-axis labels, report typed delete confirmation, comparison
  route warnings/tie state, read-only machine telemetry, and New Job
  artifact-interval fallback copy. The final surface browser pass recorded 3
  checks, 3 screenshots, zero failed API responses, and zero browser console
  errors.

Latest authenticated API evidence from 2026-06-02:

* report `bbc5d6e7-c020-41b1-8c53-58715205c95d` is `available`.
* report summary schema is `2`.
* report summary diagnostics include `source_replay_result_mismatch`.
* source net PnL is `35.86674472`.
* replay net PnL is `45.625038520000004`.
* regenerated chart data starts at job timestamp `1780259776570`.
* final chart warning check on 2026-06-02 recorded zero browser warnings and
  zero failed requests on current report detail, report comparison, and Research
  > Machines:
  `data/experiments/research-manual-qa-playwright/final-chart-and-report-mismatch-check-20260602T010914Z.json`.
* latest chart/source UI check on 2026-06-02 recorded zero browser warnings,
  zero failed requests, no missing report expectations, drawdown chart ceiling
  capped at zero, source net PnL `35.86674472`, replay net PnL `45.62503852`,
  and replay delta `9.7582938`:
  `data/experiments/research-manual-qa-playwright/report-replay-source-chart-check-20260602T013758Z.json`.
* latest local-code all-routes smoke on 2026-06-02 covered Monitor, Analysis,
  and Research routes with zero failed requests and zero console errors. The two
  recorded issues were harness expectations: authenticated `/login` redirects to
  Overview, and input placeholders do not appear in body text. Evidence:
  `data/experiments/research-manual-qa-playwright/all-routes-local-20260602T023752Z/summary.json`.
* latest local-code mobile smoke on 2026-06-02 covered key Monitor and Research
  routes at 390px width with zero horizontal body overflow, zero failed
  requests, and zero console errors. The single recorded issue was a harness
  expectation: Research Machines detail is titled `Research Hosts`. Evidence:
  `data/experiments/research-manual-qa-playwright/mobile-local-20260602T024004Z/summary.json`.

## Latest Local Verification

Passed in the current cleanup pass:

* `make lint`
* `make comment-audit`
* `make docs-audit`
* `cd dashboard/client && npm run lint`
* `cd dashboard/client && npm test`
* `cd dashboard/client && npm run build`
* direct stable `rustfmt --check` over changed Rust files
* direct stable `cargo clippy --workspace -- -D warnings`
* direct stable `cargo test -p buba-dashboard`
* direct stable `cargo test -p buba-agent`
* direct stable `cargo test -p buba-paint`
* `python3 scripts/tests/test_research_maintenance.py`
* `python3 scripts/audit-user-facing-text.py`
* non-ASCII dash search over docs and source roots, excluding historical run logs
* `git diff --check`
* `docker compose -f docker-compose.research.yml config --quiet`
* `python3 scripts/deploy-machine.py --machine research --dry-run`
* `python3 scripts/deploy-stopped-live.py --dry-run --allow-dirty-source --expected-runtime-name live-readonly-20260601-022105 --expected-db-sha256 90908f7725d2a82a5d450d07f39b18ac0bea92b4d546b651227aca31189fcd0e`
* `python3 scripts/publish-research-images.py --dry-run`
* `python3 scripts/publish-live-images.py --dry-run`

Expected local warnings:

* `cd dashboard/client && npm run build` prints the existing Vite large-bundle
  warning, but the production build completes.
* `cd dashboard/client && npm test` prints Node experimental `localStorage`
  warnings, but all 75 test files and 498 tests pass.

## Operating Rules

* Keep `buba-paint` safe. Do not start `paint` or `sidecar` unless the user
  explicitly approves a live or paper bot run.
* Use `buba.toksaitov.com` as the operator dashboard.
* Treat `testing` as research compute and storage infrastructure.
* Use Docker Compose plus scripts, not ad hoc remote commands, for deploys.
* Use digest-pinned GHCR images for deploys.
* Preserve live run backups and QA evidence on disk.
* Do not commit generated local backup directories.
