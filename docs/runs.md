# Run Index

`runs/` contains primary evidence from live paper and readonly sessions. Do not edit these DBs or logs manually. The only supported mutation is an explicit additive migration workflow such as `upgrade-history`.

Current local run sequence:

* `runs/001` through `runs/009`: early live paper runs and screenshots.
* `runs/010`: pulled run-018 live-paper artifacts used for parity work.
* `runs/011`: local and server run 011 artifacts.
* `runs/012`: archived readonly shadow run from April 2026. This was originally discussed and archived from the server as run 013, then renamed locally to keep numbering contiguous. The DB and logs were not edited.

Run 012 status:

* primary archive: `runs/012/server-20260424-183503`
* derived forensics: `data/experiments/run-012-forensics-001`
* blocked sweep note: `data/sweeps/run-012-001/SWEEP_BLOCKED.md`
* quality: descriptive only, not sweep-grade

Run 012 remains valuable for realized PnL, drawdown chain, halt behavior, strategy attribution, and operational health. It must not be used for trusted parameter selection because the original compact capture omitted Binance `bookTicker` rows required to reconstruct live decision state.

## Research Orchestration Evaluation History

This section is the dated history of the research control-plane buildout and its manual QA. The current control-plane truth lives in [research-orchestration.md](./research-orchestration.md) and the lean tracker [../RESEARCH_ORCHESTRATION.md](../RESEARCH_ORCHESTRATION.md). Detailed browser QA evidence (screenshots and per-check JSON) lives under `data/experiments/research-manual-qa-playwright/`.

Phase 15 milestone, 2026-06-11: the remote worker-controller protocol was deployed and accepted end to end from `https://buba.toksaitov.com`. A backtest job `a587567d` created from the artifact detail Backtest shortcut was claimed by `research-worker-testing` over the worker protocol, ran all six steps with live updates, and produced report `d7206c1d` with served JSON and CSV, money-axis charts, and the expected source-mismatch warning (replay `+$45.63` versus source `+$35.87`). Sweep job `bd470e04` produced report `9ec3631b` with ranked sweep rows, report comparison loaded with compatibility warnings, and a deliberately bad custom interval blocked at `prepare_backtest_input` and recovered via Clone. `paint` and `sidecar` stayed stopped throughout (verified with `live-safety`).

Honesty correction, 2026-06-11: Phase 14 unified the public view but never unified job execution. Before Phase 15 the worker on `testing` leased steps only from its own local SQLite database, the only public worker-token endpoint was heartbeat, and jobs created on the public dashboard stayed `queued` forever while worker telemetry looked healthy. The 2026-06-02 QA jobs and reports lived in the private `testing` database, not the public controller. Phase 15 added the full `/api/research/workers/` protocol, the `ResearchWorkBackend` trait with local and remote implementations, and remote worker mode so all job, transfer, report, and artifact work flows through the public controller.

Resolved findings from the manual evaluation, fixed and deployed by 2026-06-11 unless noted:

* Public job creation was a trap before Phase 15 (jobs stayed queued with a healthy-looking idle worker). Fixed by the Phase 15 protocol plus a job-detail queue-wait banner that escalates when no live worker claims a job after three minutes.
* Job detail honesty: removed the misleading Regenerate-report prompt on incomplete jobs, distinguished "no events yet" from filter mismatch, added Cancel confirmation that explains completed steps are kept, and resolved `Requested by` user ids to usernames.
* Artifact and New Job flow: dropped a duplicate status filter, sorted newest run first, added run-interval columns and direct Backtest/Sweep prefill actions, "Not assessed" quality copy, operator-phrased interval source labels, a starting-balance field, named parameter controls, full-versus-focused sweep ranges, and advanced-only database overrides.
* Long-running command confidence: supervised commands now refresh the active step lease while they run, the lease pill reads "refresh overdue" with clearer recovery guidance, and the recovery diagnosis panel scopes command output to the active target step.
* WSL/Docker compute on `testing`: verified WSL and Docker both report 32 CPUs and about 93 to 94 GiB memory, so low utilization is workload shape, not a VM cap.
* Report fidelity: schema v2 current-params reports gained a source-versus-replay comparison, a diagnostics flag, CSV rows, a detail warning, and a Reports-list "Source mismatch" flag. Report `bbc5d6e7-c020-41b1-8c53-58715205c95d` records source net PnL `35.86674472`, replay net PnL `45.62503852`, and diagnostic `source_replay_result_mismatch`. The replay still diverges from source decisions on some markets, so the comparison warning is a required review item before tuning parameters. `prepare-backtest-input` now keeps source audit rows, and sweeps run a current-params baseline and rank by `calibrated_pnl` when those rows exist.
* Report charts: the equity curve no longer includes an initial balance point at timestamp `0` (which made the x-axis span from 1970); the regenerated report starts at job timestamp `1780259776570`. Report detail charts use a padded data-domain y-axis with money tick and tooltip formatting.
* Smaller fixes: metadata-only deletes require typing the record id, login inputs are programmatically associated with their labels, the Reports list has a general `q=` search across title and ids, and the checksum toggle names the loaded `checksums.sha256` file.

Verification evidence under `data/experiments/research-manual-qa-playwright/` includes `final-chart-and-report-mismatch-check-20260602T010914Z.json`, `report-replay-source-chart-check-20260602T013758Z.json`, `finalize-public-smoke-20260602T064237Z/summary.json`, `wide-ui-evidence.json`, `template-crud-evidence.json`, `sweep-retry-evidence.json`, `public-research-followup-qa-summary-20260602T0032Z.json`, and the all-routes and mobile local smokes from 2026-06-02. Those passes recorded zero failed API responses and zero browser console errors across Monitor, Analysis, and Research routes.
