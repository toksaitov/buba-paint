# Sweep run-018-002, exact-run frontier after parity-aware settlement timing

This sweep reranks the latency-arb core frontier on the exact pulled `run-018` tape after two important parity fixes:

* observed market-resolution timing in backtest
* conservative pending-settlement reserve handling

Compared with [run-018-001](/Users/toksaitov/Desktop/buba-paint/data/sweeps/run-018-001/notes.md), this sweep is much closer to how the improved live bot will behave when a market closes but Gamma has not resolved it yet.

## Source Data

* pulled live snapshot: [runs/010](/Users/toksaitov/Desktop/buba-paint/runs/010)
* replay-compatible DB: `/tmp/run-018-replay-data.db`
* interval:
  * start: `2026-04-04T20:15`
  * end: `2026-04-08T17:25`

## Fixed Mode

All rows in this sweep used:

* `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`
* `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
* `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
* `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=0`

This is the conservative mode selected by [run-018-parity-001](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-001/notes.md).

Calm and spread were held fixed. The sweep only varied the latency axes that actually mattered on this run:

* `LATENCY_ARB_MOMENTUM_THRESHOLD`
* `LATENCY_ARB_MAX_ASK`
* `LATENCY_ARB_MAX_POSITION_FRACTION`

`SPREAD_CAPTURE_THRESHOLD` was fixed at `0.970`.

## Command

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=0 \
./target/release/buba-paint sweep \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 --end 2026-04-08T17:25 \
  --balance 200 \
  --output data/sweeps/run-018-002/sweep.csv \
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
* about `18.6 min`

## Current Live Row

Current deployed latency row:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.05`

With the parity-aware replay model, that row now scores:

* `pnl_net = $544.42`
* `trades = 169`
* `max_dd = 24.9%`

Ranking:

* raw rank: `30 / 150`
* rank with `max_dd <= 25%`: `19 / 128`
* not in the `max_dd <= 20%` set

So the current row is no longer the obviously wrong choice, but it is also clearly not the best row on this exact run once the backtester is made more realistic.

## Best Rows

Top raw row:

* `0.0008 / 0.60 / 0.125`
* `pnl_net = $1454.44`
* `max_dd = 34.0%`

Best row with `max_dd <= 25%`:

* `0.0008 / 0.60 / 0.10`
* `pnl_net = $1358.40`
* `max_dd = 24.9%`

Best row with `max_dd <= 20%`:

* `0.0008 / 0.60 / 0.075`
* `pnl_net = $1058.09`
* `max_dd = 19.1%`

The shape is clean:

* `0.0008` remains the best momentum threshold
* `0.60` is the best ask cap on this exact run
* higher latency sleeves buy more PnL, but DD rises steadily

## Recommendation

If the goal is a balanced live improvement rather than the raw-most-aggressive row, the best candidate from this sweep is:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.60`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.075`
* `SPREAD_CAPTURE_THRESHOLD=0.970`
* `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
* `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
* `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=0`

Why this row:

* it is meaningfully better than the current deployed row on exact-run replay
* it keeps DD below `20%`
* it avoids the raw-best row's jump to the edge of the `25%` DD budget

If a more aggressive live experiment is desired later, the next candidate is the same row with `LATENCY_ARB_MAX_POSITION_FRACTION=0.10`.

## Bottom Line

The important result of `run-018-002` is not just a new top row. It is that once replay honors observed settlement timing and conservative pending-settlement reserve handling, the frontier moves in a way that is much more believable than the old `run-018-001` frontier.

That makes this sweep the right basis for the next live deployment decision on `run-018`-style conditions.
