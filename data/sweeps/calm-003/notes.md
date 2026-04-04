# Calm 003, combined portfolio test with non-overlapping calm sleeve

`calm-003` is the first real portfolio test.

The question is not “is calm-persistence profitable by itself?” `calm-002` already answered that.

The real question is:

- can the calm strategy add value **without competing with** the current latency-arb control?

This pass uses the stricter `calm-002` winner, but moves it into a narrower time slice:

- calm window: `30-90s` remaining
- latency-arb window: `>90s` remaining from the existing `MIN_WINDOW_TIME_MS=90000`

That makes the portfolio split explicit rather than implicit.

## Control Reference

Current control row, unchanged:

```bash
./target/release/buba-paint backtest \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --output /tmp/calm-control.db \
  --balance 200 \
  --set LATENCY_ARB_ENABLED=1 \
  --set SPREAD_CAPTURE_ENABLED=1 \
  --set CALM_PERSISTENCE_ENABLED=0 \
  --set REGIME_DETECTION_ENABLED=0 \
  --set TREND_FILTER_PER_STRATEGY=0 \
  --set LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008 \
  --set LATENCY_ARB_MAX_ASK=0.65 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set SPREAD_CAPTURE_THRESHOLD=0.970 \
  --set SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8 \
  --set SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05 \
  --set SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set MIN_WINDOW_TIME_MS=90000
```

Control result:

- `pnl_net`: `$32,067.42`
- `trades`: `448`
- `win_rate`: `77.7%`
- `max_dd`: `19.1%`

Sanity check: turning on the router and per-strategy trend tracking **without** enabling calm leaves this control row unchanged. So the combined improvement below is not coming from a routing bug.

## Standalone Calm Sanity Check For The Combined Window

The exact calm candidate used in the combined test was also checked standalone in the same `30-90s` window:

```bash
./target/release/buba-paint backtest \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --output /tmp/calm-standalone-30-90.db \
  --balance 200 \
  --set LATENCY_ARB_ENABLED=0 \
  --set SPREAD_CAPTURE_ENABLED=0 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
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
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250
```

Standalone `30-90s` calm result:

- `pnl_net`: `$43,044.46`
- `trades`: `313`
- `win_rate`: `89.1%`
- `max_dd`: `30.0%`

That confirms the narrowed calm window still carries standalone edge.

## Combined Sweep Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --balance 200 \
  --sweep CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.01,0.02,0.03,0.05 \
  --output data/sweeps/calm-003/sweep.csv \
  --set LATENCY_ARB_ENABLED=1 \
  --set SPREAD_CAPTURE_ENABLED=1 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set TREND_FILTER_PER_STRATEGY=1 \
  --set LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008 \
  --set LATENCY_ARB_MAX_ASK=0.65 \
  --set LATENCY_ARB_MAX_POSITION_FRACTION=0.05 \
  --set MAX_POSITION_FRACTION=0.05 \
  --set SPREAD_CAPTURE_THRESHOLD=0.970 \
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
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set MIN_WINDOW_TIME_MS=90000
```

## Result

Every combined row beats the control.

### Combined rows

1. calm sleeve `0.01`
   - `pnl_net`: `$72,915.34`
   - `trades`: `826`
   - `win_rate`: `83.2%`
   - `max_dd`: `19.1%`
2. calm sleeve `0.02`
   - `pnl_net`: `$84,952.95`
   - `trades`: `837`
   - `win_rate`: `82.6%`
   - `max_dd`: `19.1%`
3. calm sleeve `0.03`
   - `pnl_net`: `$88,950.91`
   - `trades`: `842`
   - `win_rate`: `82.4%`
   - `max_dd`: `22.6%`
4. calm sleeve `0.05`
   - `pnl_net`: `$91,810.75`
   - `trades`: `857`
   - `win_rate`: `81.9%`
   - `max_dd`: `31.5%`

### Best balanced row

The best balanced row is `CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.02`.

Why:

- it raises `pnl_net` from `$32,067` to `$84,953`
- it keeps `max_dd` at the same `19.1%` as the control
- it lifts total trades from `448` to `837`

## Attribution

The combined lift is not coming from accidental strategy suppression or overlap.

For the recommended `0.02` row:

- `router_blocked_count = 0`
- `capital_blocked_count = 0`
- `latency_spread_overlap_count = 0`
- `latency_calm_overlap_count = 0`
- `spread_calm_overlap_count = 0`

Filled trades by regime:

- `dislocation_filled = 434`
- `structural_pair_filled = 12`
- `calm_filled = 391`

Missed trades by regime:

- `dislocation_missed = 170`
- `structural_pair_missed = 34`
- `calm_missed = 208`

That is exactly the pattern we wanted:

- the dislocation control remains largely intact
- the calm sleeve adds a large new block of late-window fills
- the two families are effectively non-competing in this configuration

## Interpretation

This is the first calm-strategy result that clears the real bar.

Not because the raw top PnL is huge, but because:

- the calm sleeve improves the portfolio materially
- the improvement survives under a drawdown cap
- the control strategy is not being starved
- the time split `>90s` vs `30-90s` is doing real work

The portfolio architecture matters here as much as the calm signal itself.

Without the router and sleeves, this would be much harder to trust.

## Recommendation

Promote the calm strategy program.

Recommended next candidate for any future live-paper plan:

- keep the current latency-arb control unchanged
- keep spread conservative
- use the `30-90s` calm-persistence strategy
- start with `CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.02`

Why `0.02`:

- it matches the control drawdown
- it still captures most of the combined uplift
- it is the cleanest first live-paper portfolio candidate

So the next major conclusion is:

- late-window calm persistence is not just a standalone curiosity
- it looks like a legitimate third strategy family when routed and sleeved properly
