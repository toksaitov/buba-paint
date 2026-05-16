# Spread knob 001, targeted rerun after adding spread-only sizing

This rerun exists to answer one narrow question: after adding `SPREAD_CAPTURE_MAX_POSITION_FRACTION`, should the next live run keep the aggressive `run-016` spread experiment or go back to the conservative spread settings?

The latency-arb settings were held fixed at the current recommended row:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `MAX_POSITION_FRACTION=0.05`
* `TAKER_FEE_RATE=0.072`
* `TAKER_FEE_EXPONENT=1`
* `SIM_ORDER_LATENCY_MS=250`
* `SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25`

Only the spread settings were varied:

* `SPREAD_CAPTURE_THRESHOLD=0.970,1.000`
* `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8,50`
* `SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05,0.10,0.125,0.15`

## Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --balance 200 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.970,1.000 \
  --sweep SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8,50 \
  --sweep SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05,0.10,0.125,0.15 \
  --set LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008 \
  --set LATENCY_ARB_MAX_ASK=0.65 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25 \
  --output data/sweeps/spread-knob-001/sweep.csv
```

## Result

The conservative baseline remains the winner.

* Baseline row `0.970 / 8 / 0.05`:
  * `pnl_net`: `$32,067.42`
  * `max_dd`: `19.1%`
  * `trades`: `448`
  * `fill_rate`: `68.5%`
  * `spread_legging_count`: `8`
  * `residual_position_count`: `8`
* Best row under the `max_dd <= 25%` rule:
  * exactly the same row, `0.970 / 8 / 0.05`

The `run-016` style aggressive spread experiment does not win this rerun.

* Raising `SPREAD_CAPTURE_MAX_POSITION_FRACTION` above `0.05` reduced `pnl_net` and increased drawdown.
* Relaxing `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S` from `8` to `50` made no difference on this historical replay slice.
* Raising `SPREAD_CAPTURE_THRESHOLD` from `0.970` to `1.000` also reduced `pnl_net` slightly and increased spread legging by one trade.

## Interpretation

The new spread-only sizing knob is still worth keeping. It fixes an important modeling limitation because spread sizing no longer has to share the same balance cap as latency-arb forever.

But this rerun does not support using a larger spread cap for the next live deployment.

* The current best historical row is still the conservative spread setup.
* The aggressive `run-016` behavior was not validated by the historical rerun.
* Mechanically allowing larger spread orders is not enough by itself to justify a more aggressive spread live run.

So the answer to "was the `run-016` all-rejected spread failure mode mostly removed by a larger spread cap?" is: no, not in a way that improves the historical objective.

## Deployment recommendation

Use the conservative spread settings for the next live run, but set the new knob explicitly so the behavior is not implicit:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_THRESHOLD=0.970`
* `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8`
* `SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25`
* `TAKER_FEE_RATE=0.072`
* `TAKER_FEE_EXPONENT=1`
* `SIM_ORDER_LATENCY_MS=250`

That is the right `run-017` choice if the goal is a clean live evaluation rather than another spread-activation experiment.
