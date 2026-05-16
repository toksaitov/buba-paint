# Run 012 analysis plan

Run 012 is archived locally at `runs/012/server-20260424-183503`. Treat this archive as the baseline evidence for the live_readonly shadow run that ended after the max drawdown hard stop. This local run was originally discussed as server run 013 before local renumbering.

The next step should be analysis before behavior-changing fixes. The value of this run is that it captures the exact strategy, sizing, feed, execution, and risk logic that produced both the strong early PnL and the later drawdown. If we change trading behavior first, it becomes harder to separate evidence from new assumptions.

Safe work before analysis:

* Add read-only analysis scripts or notebooks that do not mutate the archived DB.
* Add labels and reporting that make existing behavior easier to understand.
* Add dashboard/API presentation for already-existing states, as long as it does not change trading or backtest semantics.
* Add documentation and incident-report templates.

Avoid before baseline analysis:

* Strategy threshold changes.
* Position sizing changes.
* Drawdown policy changes.
* Pending-settlement reserve changes.
* Feed freshness gate changes.
* Execution fill or settlement behavior changes.

Primary analysis questions:

* Which strategy families produced the run-level PnL, and which families drove the drawdown?
* Did the drawdown come from one market regime, one time period, one strategy, or broad degradation?
* Did risky pending-settlement reserve settings increase effective exposure during the losing phase?
* How much of the loss came from latency-arb, calm-persistence, and spread-capture separately?
* Were losing entries concentrated near specific prices, window times, volatility regimes, fee profiles, or quote churn levels?
* Did CLOB feed churn or stale-feature rejection correlate with reduced opportunity quality or missed fills?
* Did strategy_sleeve_exhausted after the drawdown hide a clearer max_drawdown_exceeded halt reason?
* How different were the profitable and losing periods in feature distributions?
* Would simple risk changes have reduced drawdown without destroying most of the prior PnL?

Suggested analysis order:

1. Build a run summary from the archived DB: balance curve, high-water mark, max drawdown, trade count, win rate, total fees, strategy contribution, and last active state.
2. Split the run into phases: ramp-up, peak, drawdown, halted-after-drawdown.
3. Attribute PnL by strategy family, market, side, entry price bucket, window time bucket, and execution fidelity.
4. Inspect the losing phase trade by trade, including nearby signals and rejection summaries.
5. Compare feature distributions for winning trades, losing trades, queued signals, and rejected signals.
6. Quantify feed health: reconnects, parse failures, stale features, book staleness, and quote age around the drawdown phase.
7. Run exact-baseline replays where possible to confirm that the archived DB and current code still reproduce expected aggregates.
8. Run parameter sweeps only after the baseline report is stable.

Sweep priorities:

* Max position fraction and per-family position fractions.
* Max drawdown threshold and peak drawdown pause behavior.
* Pending-settlement reserve mode and fractions.
* Calm-persistence max ask, min distance, volatility, and alignment filters.
* Latency-arb momentum threshold, cooldown, max ask, and adaptive window.
* Spread-capture threshold, max leg skew, and legging-risk gates.

Outputs to produce:

* A concise run 012 postmortem.
* A baseline metrics JSON or Markdown report that future code can compare against.
* A ranked list of strategy and risk changes with expected tradeoffs.
* A short recommendation on whether the next live_readonly run should keep the same strategy set or disable any family.
