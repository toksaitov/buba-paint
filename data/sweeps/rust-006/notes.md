# Sweep rust-006, full date range (Feb 15 to Mar 20)

Date: 2026-03-20
Status: VALID, first sweep covering all available data at the time

## Purpose

Extended the sweep from the 8-day window (Feb 20-28) used in sweeps 001-005 to the full data range available: Feb 15 to Mar 20, spanning runs 004-007 and 8.8M ticks. Tests whether the patterns found in the short window hold across a longer period.

## Parameters

* Data: `data/market-data.db` (runs 004-007)
* Time range: 2026-02-15 to 2026-03-20 (769.3h, ~32 days)
* Ticks: 8,788,883
* Balance: $200
* Swept:
  * `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.001 to 0.003, step 0.0002 (11 values)
  * `LATENCY_ARB_MAX_ASK`: 0.45, 0.50, 0.55, 0.60, 0.65 (5 values)
  * `MAX_POSITION_FRACTION`: 0.05, 0.075, 0.10, 0.125, 0.15 (5 values)
* Fixed:
  * `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
  * `PEAK_DD_PAUSE_PCT=1.0` (disables peak drawdown pause)
* Total: 275 combinations
* Runtime: ~4 min

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-15 --end 2026-03-20 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-006/sweep.csv
```

## Top 5 by PnL

1. mom=0.0012, ask=0.60, frac=0.10: $815,678, 57.8% WR, 929 trades, 49.7% DD
2. mom=0.0012, ask=0.50, frac=0.15: $547,631, 53.3% WR, 516 trades, 53.2% DD
3. mom=0.0012, ask=0.50, frac=0.125: $332,740, 53.2% WR, 517 trades, 51.0% DD
4. mom=0.0012, ask=0.60, frac=0.075: $303,344, 57.8% WR, 929 trades, 43.8% DD
5. mom=0.0012, ask=0.50, frac=0.10: $217,484, 53.0% WR, 564 trades, 47.4% DD

## Key findings

1. mom=0.0012 still dominates, same as in all prior sweeps. Top 5 results all use this value.

2. The PnL numbers are much larger than rust-004/005 ($815k vs $4k) because half-Kelly compounding over 32 days produces exponential growth. This is paper trading; the absolute numbers are meaningless, but the relative ranking matters.

3. ask=0.50-0.60 works best over the full range (ask=0.50 was weak in the 8-day window but strong here, suggesting it picks up more opportunities in certain market conditions).

4. Higher fraction = higher PnL but also higher DD. frac=0.075 keeps DD under 44% while still producing $303k; frac=0.10-0.15 pushes DD to 50%+.

5. Win rates are lower than the 8-day sweeps (53-58% vs 59-61%) because the longer window includes both favorable and unfavorable market regimes.
