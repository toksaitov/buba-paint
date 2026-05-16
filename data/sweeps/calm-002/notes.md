# Calm 002, focused standalone refinement around the viable calm cluster

`calm-001` established that the first calm parameterization was too strict to trade. `calm-002` is the focused refinement pass after loosening the hypothesis and finding the viable region.

This is still standalone calm research:

* latency-arb disabled
* spread disabled
* router enabled for regime attribution

## Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --balance 200 \
  --sweep CALM_PERSISTENCE_MAX_ASK=0.65,0.75 \
  --sweep CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=5,6 \
  --sweep CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0,1.5 \
  --sweep CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.50,0.60 \
  --sweep CALM_PERSISTENCE_MAX_FAIR_BIAS=0.25,0.35 \
  --set LATENCY_ARB_ENABLED=0 \
  --set SPREAD_CAPTURE_ENABLED=0 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000 \
  --set CALM_PERSISTENCE_MAX_WINDOW_TIME_MS=120000 \
  --set CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=80 \
  --set CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=1 \
  --set CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=100 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/calm-002/sweep.csv
```

## Result

This pass found the first real calm frontier.

* rows: `32`
* zero-trade rows: `16`

That split is already informative: half the grid is dead, half is very live.

The dead half is clean:

* `MIN_ALIGNMENT_FRACTION=0.60` kills the strategy outright
* every `0.60` row produced `0` trades and `0` PnL

So the first major conclusion is:

* the calm signal only survives at the looser `0.50` alignment threshold

The profitable half is also clean:

* `MIN_ALIGNMENT_FRACTION=0.50`
* distance filter `5-6 bps`
* volatility ratio threshold `1.0-1.5`
* fair-bias cap `0.35`

### Best raw standalone row

* `CALM_PERSISTENCE_MAX_ASK=0.75`
* `CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=5`
* `CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0`
* `CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.50`
* `CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35`

Result:

* `pnl_net`: `$58,602.79`
* `trades`: `483`
* `win_rate`: `82.6%`
* `max_dd`: `45.0%`
* `signals`: `1,429`
* `fill_rate`: `42.6%`

### Best row under a stricter `max_dd <= 40%` cap

* `CALM_PERSISTENCE_MAX_ASK=0.75`
* `CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6`
* `CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0`
* `CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.50`
* `CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35`

Result:

* `pnl_net`: `$56,075.25`
* `trades`: `410`
* `win_rate`: `86.6%`
* `max_dd`: `39.6%`
* `signals`: `1,260`
* `fill_rate`: `41.2%`

## Interpretation

`calm-002` says the calm-persistence idea is real, but it is not broad.

The strategy is very sensitive to exactly two filters:

* alignment
* minimum distance from open

What matters most:

* pushing alignment from `0.50` to `0.60` is not a small degradation, it is a full shutdown
* increasing minimum distance from `5` to `6` reduces activity, but it also reduces drawdown materially
* `MAX_ASK=0.65` and `0.75` are close; `0.75` is slightly better on the frontier
* the `1.0` and `1.5` distance/vol thresholds are effectively tied in this narrowed region

The right reading is:

* there is a real calm-market edge here
* but the standalone version is still too high-DD to ship directly

## Selection For Combined Testing

Use the stricter row for `calm-003`, not the raw standalone winner:

* `CALM_PERSISTENCE_MAX_ASK=0.75`
* `CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6`
* `CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0`
* `CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.50`
* `CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35`

Reason:

* it keeps almost all of the raw edge
* it reduces standalone drawdown from `45.0%` to `39.6%`
* it is the better research candidate for a sleeved combined portfolio

## Decision

Promote the stricter `distance=6` row to `calm-003`.

Do not promote the raw standalone winner directly. It is too hot to judge without sleeves and non-overlap routing.
