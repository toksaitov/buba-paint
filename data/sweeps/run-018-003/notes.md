# Sweep run-018-003, release-candidate frontier after the default-mode and docs cleanup

This is the final exact-run sweep after:

* making conservative pending-settlement handling the real default
* centralizing reserve-mode semantics in config
* tightening docs and operator defaults
* rechecking the short ladder in [run-018-parity-002](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-002/notes.md)

The purpose of this sweep is not to discover a new idea. It is to confirm that the release-candidate frontier still holds on the cleaned-up codebase.

## Source Data

* pulled live snapshot: [runs/010](/Users/toksaitov/Desktop/buba-paint/runs/010)
* replay-compatible DB: `/tmp/run-018-replay-data.db`
* interval:
  * start: `2026-04-04T20:15`
  * end: `2026-04-08T17:25`

## Fixed Mode

All rows in this sweep used:

* `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`
* conservative pending-settlement reserve handling, now the default:
  * `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
  * `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
  * `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false`

Calm and spread stayed fixed. The sweep only varied the latency-arb axes that mattered on this run:

* `LATENCY_ARB_MOMENTUM_THRESHOLD`
* `LATENCY_ARB_MAX_ASK`
* `LATENCY_ARB_MAX_POSITION_FRACTION`

`SPREAD_CAPTURE_THRESHOLD` remained fixed at `0.970`.

## Command

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
./target/release/buba-paint sweep \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 --end 2026-04-08T17:25 \
  --balance 200 \
  --output data/sweeps/run-018-003/sweep.csv \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep LATENCY_ARB_MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.970 \
  --set LATENCY_ARB_ENABLED=1 \
  --set SPREAD_CAPTURE_ENABLED=1 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set TREND_FILTER_PER_STRATEGY=1 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8 \
  --set SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05 \
  --set SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25 \
  --set CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000 \
  --set CALM_PERSISTENCE_MAX_WINDOW_TIME_MS=90000 \
  --set CALM_PERSISTENCE_MAX_ASK=0.75 \
  --set CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6 \
  --set CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0 \
  --set CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.5 \
  --set CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35 \
  --set CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=80 \
  --set CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=1 \
  --set CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=100 \
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set MIN_WINDOW_TIME_MS=90000
```

Runtime:

* `150` rows
* about `26.5 min`

## Current Row

Current deployed latency row:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.05`

On the cleaned-up parity-aware model, that row scores:

* `pnl_net = $544.42`
* `trades = 169`
* `max_dd = 24.9%`

Ranking:

* raw rank: `30 / 150`
* rank with `max_dd <= 25%`: `19 / 128`

## Best Rows

Top raw row:

* `0.0008 / 0.60 / 0.125`
* `pnl_net = $1453.57`
* `max_dd = 34.0%`

Best row with `max_dd <= 25%`:

* `0.0008 / 0.60 / 0.10`
* `pnl_net = $1358.40`
* `max_dd = 24.9%`

Best row with `max_dd <= 20%`:

* `0.0008 / 0.60 / 0.075`
* `pnl_net = $1058.09`
* `max_dd = 19.1%`

The shape is unchanged from `run-018-002`:

* `0.0008` remains the best momentum threshold
* `0.60` remains the best ask cap
* larger latency sleeves still buy more PnL, but drawdown climbs in a smooth, predictable way

## Release-Candidate Recommendation

The release-candidate block stays:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.60`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.075`
* `SPREAD_CAPTURE_THRESHOLD=0.970`
* conservative pending-settlement handling:
  * `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
  * `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
  * `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false`

Why this row:

* materially stronger than the current deployed row on exact-run replay
* still below the `20%` drawdown line
* keeps the upside from the reserve fix without pushing all the way to the `0.10` sleeve row

## Bottom Line

This rerun did what it needed to do:

* the cleanup did not move the exact-run frontier in a surprising way
* the default-mode switch is now aligned with the preferred operating mode
* the next local release candidate is supported by the cleaned-up code, the cleaned-up docs, and a fresh exact-run frontier, not just by older notes
