# Experiment run-018-parity-001, calibrating live-vs-backtest reserve timing on the exact `run-018` tape

This experiment answers one practical question:

* can we make replay of the pulled `run-018` behave enough like the real live bot to rank latency-arb settings with more confidence?

The starting problem was large:

* real live `run-018` snapshot: about `-$9.05` total realized PnL on `44` settled trades
* old exact-run replay of the same row: about `+$558.11` on `170` trades

That gap was too large to treat as normal replay noise.

## Source Data

* pulled live DB and log: [runs/010](/Users/toksaitov/Desktop/buba-paint/runs/010)
* replay-compatible exact-run DB: `/tmp/run-018-replay-data.db`
* frozen interval:
  * start: `2026-04-04T20:15`
  * end: `2026-04-08T17:25`

The live result used for comparison came from the pulled snapshot:

* total `pnl_net`: about `-$9.05`
* settled trades: `44`
* by strategy:
  * `latency-arb`: `38` trades, about `-$17.17`
  * `calm-persistence`: `6` trades, about `+$8.12`
  * `spread-capture`: `0`

## Why The Gap Existed

Before this work, the biggest live-vs-backtest difference on this run was capital timing:

* live held reserve until authoritative Polymarket settlement
* backtest released reserve immediately at market close

On the pulled live run:

* latency-arb had `55` `strategy_sleeve_exhausted` rejections
* all `55` happened while another latency-arb trade was already past market end
* `53` of those also overlapped with a calm trade that was already past market end

So the live bot was not mostly choking on active market risk. It was choking on closed-market settlement lag.

## What Changed

The code now supports two related ideas:

1. Phase-aware unresolved-trade reserve:
   * `active_market`
   * `pending_settlement`

2. Replay of observed settlement timing for exact pulled runs:
   * `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`

The reserve policy is config-driven:

* `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION`
* `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION`
* `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION`

Three modes were evaluated:

* compatibility: `1.0 / 1.0 / true`
* conservative: `0.0 / 1.0 / false`
* risky: `0.0 / 0.25 / false`

## Calibration Ladder

Results from [calibration.tsv](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-001/calibration.tsv):

* current row, old immediate settlement:
  * `+$558.11`
  * `170` trades
  * `0` latency sleeve exhaustions
* current row, observed settlement + compatibility reserve mode:
  * `+$352.23`
  * `118` trades
  * `1` latency sleeve exhaustion
* current row, observed settlement + conservative mode:
  * `+$544.42`
  * `169` trades
  * `0` latency sleeve exhaustions
* current row, observed settlement + risky mode:
  * identical to conservative on this exact run

This tells us two things:

1. `observed_market_resolution` materially reduces the optimism gap.
2. Releasing the family sleeve at market close recovers almost all of the lost opportunity without taking the extra global-reserve haircut risk.

It does not fully close the gap to real live PnL. That is expected. The observed-resolution map only contains markets that actually traded live, so replay-only markets still use a fallback delay. Real live also includes operational timing effects that no pulled-run replay can reconstruct perfectly.

## Shortlist

Five candidate latency rows were then compared under conservative and risky modes. Calm and spread settings were kept fixed.

Results from [shortlist.tsv](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-001/shortlist.tsv):

* current row `0.0008 / 0.65 / 0.05 / 0.970`
  * `+$544.42`, DD `24.9%`
* ask-only neighbor `0.0008 / 0.60 / 0.05 / 0.965`
  * `+$704.47`, DD `12.9%`
* best prior `<= 25%` DD row `0.0008 / 0.60 / 0.075 / 0.965`
  * `+$1058.09`, DD `19.1%`
* raw-best prior row `0.0008 / 0.60 / 0.10 / 0.965`
  * `+$1358.40`, DD `24.9%`
* best prior `<= 20%` DD alternative `0.0012 / 0.70 / 0.10 / 0.965`
  * `+$695.12`, DD `18.3%`

Conservative and risky were identical on all `5 / 5` shortlist rows.

By the rule set in the plan, that means:

* conservative mode wins by default
* risky mode is not justified on this run

## Interpretation

The important outcome is not "make the live bot gamble harder." It is:

* stop charging strategy sleeves for risk that is already over
* keep global capital locked conservatively until authoritative settlement

That is exactly what the conservative mode does.

For this exact run, the best balanced candidate after the parity work is:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.60`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.075`
* conservative pending-settlement mode:
  * `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
  * `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
  * `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=0`

The more aggressive `0.10` sleeve row is better on raw PnL, but it spends essentially the full `<= 25%` DD budget.

## Promote / Kill

* Promote:
  * conservative pending-settlement mode
  * exact-run parity work based on `observed_market_resolution`
* Kill for now:
  * risky pending-settlement global-reserve haircut on live
  * immediate-settlement exact-run calibration as a deployment oracle

## Next Step

The final exact-run sweep after this calibration is [run-018-002](/Users/toksaitov/Desktop/buba-paint/data/sweeps/run-018-002/notes.md). That sweep fixes settlement mode and reserve mode to the chosen conservative parity model and re-ranks the latency frontier on the exact `run-018` tape.
