# Run 012 improvement plan

This plan records the local code improvements identified during the run 012 shutdown and archive work. It is intentionally separate from the analysis plan because most items below can change behavior or operator experience. This local run was originally discussed as server run 013 before local renumbering.

## Principles

* Treat a max drawdown breach as a terminal run state, not as a normal empty-trading condition.
* Keep process liveness separate from trading readiness. A bot can be running while trading is halted.
* Do not give the operator a convenient revenge-trading path after a hard stop.
* Preserve run data first. Analyze next. Change trading behavior only after evidence.
* Make degraded states explicit in API, logs, and dashboard UI. Avoid forcing operators to infer risk state from raw logs.

## Risk halt semantics

Current behavior blocks trading when drawdown from high-water mark reaches `MAX_DRAWDOWN_PCT`. That is effectively a hard stop unless pending settlement or a correction later raises balance below the threshold again.

Improvements:

* Add an explicit halt reason such as `max_drawdown_exceeded`.
* Stop reporting max-drawdown blocks as `strategy_sleeve_exhausted`.
* Persist or expose the halt state with breach time, balance, high-water mark, drawdown percent, configured limit, and last trade.
* Add tests for the exact boundary: just below the limit trades, at the limit halts, above the limit halts.
* Add tests proving post-halt candidates are rejected with the explicit max-drawdown reason.

## Dashboard hard-stop UX

The dashboard should make a risk halt impossible to miss.

Improvements:

* Header: show `Process running` separately from `Trading halted`.
* Overview and Execution: show a red halt panel when a hard stop is active.
* Halt panel fields: breach time, current balance, high-water mark, drawdown percent, configured max drawdown, last trade, open trades, and whether collectors are still running.
* Do not expose a one-click resume button after a hard stop.
* Safe actions only: export run data, generate incident report, archive run, stop collectors.
* Restart should be a deliberate new-run workflow with config review, not a dashboard action on the halted run.

## Bot loop stall hardening

During shutdown investigation, the process had been alive and feed tables were still receiving ticks, but window activation and strategy logs stopped. This means feed ingestion can appear healthy while the main market or strategy loop is stale.

Improvements:

* Add live-loop heartbeat fields: last window activation, last market close, last strategy cycle, last rejection rollup, last feed tick, last DB write, and last log emit.
* Expose heartbeat health through agent status or trading summary.
* Add alerting when feeds are fresh but the strategy/window loop is stale.
* Instrument slow sections in the live loop, especially market close handling, rejection rollup persistence, and large DB reads.
* Add tests for stale strategy loop detection independent from feed freshness.

## Rejection rollup bounding

The logs showed large rejection rollups for old markets, including stale-feature summaries long after those markets were no longer active. This creates noisy logs and may create avoidable periodic work.

Improvements:

* Bound periodic rejection snapshots to active and recently closed markets.
* Drain and remove closed-market rejection state deterministically.
* Cap rollup log volume per timer tick.
* Keep persisted summaries useful, but avoid repeatedly logging old markets.
* Add tests that old closed markets do not reappear in periodic rollups.

## CLOB feed and parser robustness

CLOB churn did not appear to be the primary cause of the drawdown halt, but it adds noise and can contribute to stale-feature rejections or missed opportunity quality.

Improvements:

* Classify parser failures by stable reason.
* Count parse failures, idle timeouts, reconnects, resubscriptions, and book reset events separately.
* Preserve last good book state only within strict freshness limits.
* Avoid reconnect storms when repeated token resubscription or idle timeouts happen close together.
* Add tests for malformed CLOB frames, partial book updates, duplicated messages, stale book state, and reconnect coalescing.

## Replay-grade data capture

The run 012 parity pass proved that compact feed-event storage is not sufficient for exact parameter sweeps. The live runtime used Binance `bookTicker` messages in memory, but compact storage persisted zero Binance `bookTicker` rows. Current replay can reconstruct signed trades, depth summaries, CLOB books, and native window opens, but it cannot reconstruct the missing book-ticker decision cadence.

Improvements:

* Add a research-run storage profile that persists Binance `bookTicker` rows, or a compact derived stream that preserves `SignalState.binance_book` exactly enough for replay.
* Add a startup log and DB metadata row that records whether a run is sweep-grade, descriptive-only, or legacy-snapshot only.
* Add a pre-sweep guard that refuses exact optimization when required decision-triggering feed classes are absent.
* Keep compact production storage available, but do not use compact archives as exact parameter-sweep inputs unless the missing state is intentionally persisted elsewhere.
* Add tests proving replay-grade archives reproduce `raw_event_full` features and decision cadence from persisted data.

## Sidecar reconnect noise

The sidecar was ready after hardening, but user-stream reconnect logs were frequent. That is mostly observability debt, not a known money-safety failure.

Improvements:

* Aggregate benign reconnects into rollups.
* Keep raw details available for debugging without flooding normal logs.
* Alert only when reconnect frequency crosses a threshold or account freshness degrades.
* Add health fields that distinguish harmless reconnect churn from degraded account truth.
* Add tests for reconnect rollup thresholds and degraded account freshness.

## Incident report workflow

When a run halts, the system should guide the operator toward analysis.

Improvements:

* Add a command or script that generates a run incident report from DB, logs, config metadata, and final risk state.
* Include final balance, high-water mark, drawdown breach, strategy contribution, top losing windows, feed health summary, sidecar readiness summary, and archive path.
* Keep this read-only against archived run data.
* Use the report as the handoff into parameter sweeps and strategy review.

## Recommended implementation order

1. Run 012 analysis and baseline reports.
2. Replay-grade data capture and pre-sweep guards.
3. Risk halt semantics and explicit `max_drawdown_exceeded` labeling.
4. Dashboard hard-stop UX.
5. Bot-loop heartbeat and stale-loop detection.
6. Rejection rollup bounding.
7. CLOB feed and parser robustness.
8. Sidecar reconnect noise reduction.
9. Incident report command and export workflow.
