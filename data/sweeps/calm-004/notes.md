# Calm 004, post-fix confirmation of calm ask cap and minimum expected edge

`calm-004` re-runs calm-only historical research after the calm execution-path fixes from the `run-011` forensic pass.

This sweep is intentionally narrow. It only re-tests the two calm knobs that the live data challenged:

* `CALM_PERSISTENCE_MAX_ASK`
* `CALM_PERSISTENCE_MIN_EXPECTED_EDGE`

Everything else stays pinned to the previously viable calm row.

## Command

```bash
BACKTEST_SETTLEMENT_MODE=immediate \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false \
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --balance 200 \
  --sweep CALM_PERSISTENCE_MAX_ASK=0.60,0.65,0.75 \
  --sweep CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.00,0.03,0.05,0.08 \
  --set LATENCY_ARB_ENABLED=false \
  --set SPREAD_CAPTURE_ENABLED=false \
  --set CALM_PERSISTENCE_ENABLED=true \
  --set REGIME_DETECTION_ENABLED=true \
  --set TREND_FILTER_PER_STRATEGY=true \
  --set MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000 \
  --set CALM_PERSISTENCE_MAX_WINDOW_TIME_MS=90000 \
  --set CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6 \
  --set CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0 \
  --set CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.50 \
  --set CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35 \
  --set CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=80 \
  --set CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=1 \
  --set CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=100 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set MIN_WINDOW_TIME_MS=90000 \
  --output data/sweeps/calm-004/sweep.csv
```

## Result

Rows: `12`

Top rows by PnL:

1. `MAX_ASK=0.65`, `MIN_EXPECTED_EDGE=0.00` -> `+$47,944.53`, `405` trades, `26.3%` max DD
2. `MAX_ASK=0.60`, `MIN_EXPECTED_EDGE=0.00` -> `+$46,991.90`, `362` trades, `27.3%` max DD
3. `MAX_ASK=0.65`, `MIN_EXPECTED_EDGE=0.03` -> `+$46,698.48`, `391` trades, `25.3%` max DD
4. `MAX_ASK=0.60`, `MIN_EXPECTED_EDGE=0.03` -> `+$45,329.15`, `353` trades, `27.3%` max DD
5. `MAX_ASK=0.65`, `MIN_EXPECTED_EDGE=0.05` -> `+$44,824.38`, `375` trades, `21.1%` max DD

Important loser from the old worldview:

* `MAX_ASK=0.75`, `MIN_EXPECTED_EDGE=0.00` only reaches `+$43,796.78` with `34.5%` max DD

## Interpretation

The post-fix historical frontier shifts in two useful ways.

First, `0.75` is no longer the best calm ask cap. Once calm actually gets its own ask cap in the executor, `0.65` is the best broad historical region and `0.60` is close behind.

Second, the new expected-edge floor is a real drawdown control. `MIN_EXPECTED_EDGE=0.05` on the `0.65` row gives up some raw PnL versus `0.00`, but it materially improves the historical drawdown profile:

* `0.65 / 0.00` -> `26.3%` DD
* `0.65 / 0.05` -> `21.1%` DD

That makes `0.65 / 0.05` the best historical compromise row in this narrowed sweep.

## Takeaway

`calm-004` does not support keeping the old `0.75` calm ask cap.

The best broad historical compromise after the calm fix is:

* `CALM_PERSISTENCE_MAX_ASK=0.65`
* `CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.05`

That is the row taken forward into the exact `run-011` combined confirmation.
