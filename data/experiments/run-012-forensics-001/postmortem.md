# Run 012 Forensic Postmortem

This postmortem is descriptive only, not sweep-grade. The archive is useful for realized PnL, drawdown, halt behavior, strategy attribution, and operational health. It must not be used for trusted parameter optimization because compact capture omitted Binance `bookTicker` rows that live decisions used in memory. This local run was originally discussed as server run 013 before local renumbering.

## Executive Read

Run 012 grew from `$99.17` to a final archived balance of `$701.50`. The run was strongly positive in absolute PnL, but it hit the configured hard drawdown boundary after a large peak-to-trough giveback from `$1,404.22` at `2026-04-21 18:55:22 UTC` to `$701.50` at `2026-04-23 03:10:14 UTC`.

The breach was `50.04%` against `MAX_DRAWDOWN_PCT=50%`. The process continued collecting feed and signal data after the halt, but execution was effectively stopped. Current code reports those later capital blocks through `strategy_sleeve_exhausted`, which is operationally misleading and should become an explicit `max_drawdown_exceeded` state.

## Strategy Contribution

* `latency-arb`: `110` trades, `69` wins, `41` losses, pnl `$804.71`, avg `$7.32`, worst `$-151.87`, best `$166.99`, fees `$61.97`
* `spread-capture`: `3` trades, `0` wins, `3` losses, pnl `$-61.77`, avg `$-20.59`, worst `$-34.06`, best `$-5.32`, fees `$0.92`
* `calm-persistence`: `67` trades, `37` wins, `30` losses, pnl `$-140.61`, avg `$-2.10`, worst `$-46.39`, best `$71.58`, fees `$22.29`

## Why Positive Latency-Arb Did Not Save The Run

Latency-arb was positive overall, but the run-level risk was path-dependent. The account reached a high-water mark above the final balance, then a cluster of losses reduced equity by roughly half from that peak. A strategy can have positive total contribution and still leave the portfolio halted if sizing, timing, and correlated late losses drive the peak-to-trough path through the hard stop.

## Rejection and Signal Quality

* `spread-capture` / `legging_risk_too_high`: `460,668,604` rejects across `3,105` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:26:33 UTC`
* `calm-persistence` / `calm_window_inactive`: `458,602,845` rejects across `3,389` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:30:00 UTC`
* `latency-arb` / `direction_not_selected`: `389,819,098` rejects across `2,845` rollups from `2026-04-12 15:40:00 UTC` to `2026-04-24 18:26:33 UTC`
* `latency-arb` / `window_too_late`: `168,037,600` rejects across `3,335` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:26:33 UTC`
* `spread-capture` / `spread_threshold_not_met`: `80,343,327` rejects across `3,061` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:26:33 UTC`
* `calm-persistence` / `distance_below_threshold`: `67,659,723` rejects across `1,987` rollups from `2026-04-12 15:40:00 UTC` to `2026-04-24 18:15:00 UTC`
* `calm-persistence` / `entry_ask_above_max`: `41,235,220` rejects across `1,928` rollups from `2026-04-12 15:45:00 UTC` to `2026-04-24 18:26:33 UTC`
* `spread-capture` / `entry_ask_below_min`: `19,050,583` rejects across `2,655` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:26:33 UTC`
* `latency-arb` / `cooldown_active`: `14,019,112` rejects across `251` rollups from `2026-04-12 22:35:00 UTC` to `2026-04-24 16:26:37 UTC`
* `spread-capture` / `non_positive_quotes`: `12,569,798` rejects across `2,181` rollups from `2026-04-12 15:35:00 UTC` to `2026-04-24 18:26:33 UTC`
* `calm-persistence` / `non_positive_quotes`: `6,672,210` rejects across `841` rollups from `2026-04-12 16:05:00 UTC` to `2026-04-24 18:26:33 UTC`
* `spread-capture` / `features_stale`: `3,394,422` rejects across `3,336` rollups from `2026-04-12 15:40:00 UTC` to `2026-04-24 18:30:00 UTC`

## Live Readonly Account Snapshot

* snapshots: `14344` from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:34:53 UTC`
* cash available range: `$99.17` to `$99.17`
* total equity range: `$99.17` to `$99.17`

## Baseline Replay Status

* exact replay final balance: `$183.65`
* final balance delta versus archive: `$-517.85`
* trade count delta: `-44`
* max drawdown delta: `-13.36pp`

## Next Decisions

* Do not use raw PnL alone for parameter selection. Rank candidates by PnL, max drawdown, drawdown phase behavior, trade count, and strategy concentration.
* Fix `max_drawdown_exceeded` labeling and dashboard hard-stop UX after analysis, not before the baseline comparison.
* Treat CLOB churn and sidecar reconnect noise as hardening work. They were not the primary no-trade cause, but they are relevant to trust and operator noise.
