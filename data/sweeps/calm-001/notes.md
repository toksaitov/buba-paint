# Calm 001, first standalone late-window persistence scan

`calm-001` exists to answer the first question only:

* does the late-window sign-persistence idea produce any standalone candidate flow at all?

At this stage, latency-arb and spread were disabled on purpose. The point was attribution purity, not portfolio quality.

## Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --balance 200 \
  --sweep CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000,45000,60000 \
  --sweep CALM_PERSISTENCE_MAX_ASK=0.55,0.60,0.65 \
  --sweep CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=4,6,8 \
  --sweep CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.25,1.75,2.25 \
  --sweep CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=0,1,2 \
  --set LATENCY_ARB_ENABLED=0 \
  --set SPREAD_CAPTURE_ENABLED=0 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
  --set CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=40 \
  --set CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=25 \
  --set CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.60 \
  --set CALM_PERSISTENCE_MAX_FAIR_BIAS=0.18 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/calm-001/sweep.csv
```

## Result

The first pass was too strict.

* rows: `243`
* max signals: `0`
* max calm candidates: `0`
* calm regime detections were not zero:
  * `calm_regime_count` ranged from `263,441` to `591,240`

So this was not a plumbing or router failure. The calm regime was being detected. The strategy simply never found a candidate that survived the initial filters.

## Interpretation

`calm-001` killed the naive first parameterization.

The strongest signal here is not “the idea is dead.” It is:

* the default calm feature stack was too strict all at once
* especially the combination of distance, volatility normalization, and alignment
* the strategy needed a looser diagnostic pass before a real refinement sweep

That diagnostic check was run outside this numbered sweep series and confirmed the plumbing:

* with heavily relaxed gates, the calm strategy did trade
* but the relaxed version was unprofitable and high-DD

That is exactly the result you want from a first research pass:

* no false confidence
* no hidden engine bug
* a clear direction for the next attempt

## Decision

Kill the `calm-001` parameter region.

Move to a narrower second attempt that keeps the core hypothesis but relaxes the gates enough to discover where the real candidate cluster lives.
