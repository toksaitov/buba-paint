# Sweep 003, clean baseline (DD pause disabled)

Date: 2026-03-13
Status: VALID, first trustworthy sweep baseline

## Parameters

Same grid as sweeps 001/002 with peak DD pause disabled.

* Data: `data/market-data.db` (runs 004-007)
* Time range: 2026-02-20T03:13 -> 2026-02-28T00:00 (188.8h)
* Ticks: 2,710,038
* Balance: $200
* Swept:
  * `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.001 -> 0.003, step 0.0002 (11 values)
  * `LATENCY_ARB_MAX_ASK`: 0.45, 0.50, 0.55, 0.60, 0.65 (5 values)
  * `MAX_POSITION_FRACTION`: 0.05, 0.075, 0.10, 0.125, 0.15 (5 values)
* Fixed:
  * `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
  * `PEAK_DD_PAUSE_PCT=1.0` (disables peak drawdown pause)
* Total: 275 combinations
* Runtime: 39.7 min (2,383s)

## Command

```bash
npm run sweep -- \
  --data data/market-data.db \
  --output data/sweeps/003/sweep.csv \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 \
  --set PEAK_DD_PAUSE_PCT=1.0
```

## Top 10 by PnL

| # | mom | ask | frac | PnL | WR | Trades | MaxDD | Peak |
|---|---|---|---|---|---|---|---|---|
| 1 | 0.0012 | 0.65 | 0.125 | $4,070 | 60.9% | 284 | 46.4% | $4,741 |
| 2 | 0.0012 | 0.60 | 0.15 | $3,490 | 59.2% | 238 | 44.4% | $5,019 |
| 3 | 0.0012 | 0.65 | 0.10 | $3,386 | 60.9% | 284 | 40.3% | $3,808 |
| 4 | 0.0012 | 0.60 | 0.125 | $3,360 | 59.2% | 238 | 42.8% | $4,690 |
| 5 | 0.0012 | 0.60 | 0.10 | $2,893 | 59.2% | 238 | 39.0% | $3,729 |
| 6 | 0.0016 | 0.65 | 0.15 | $2,413 | 63.3% | 150 | 45.7% | $3,126 |
| 7 | 0.0012 | 0.65 | 0.075 | $2,172 | 60.9% | 284 | 32.3% | $2,453 |
| 8 | 0.0016 | 0.65 | 0.125 | $2,060 | 63.3% | 150 | 42.6% | $2,599 |
| 9 | 0.0014 | 0.60 | 0.125 | $2,033 | 59.7% | 176 | 48.3% | $2,712 |
| 10 | 0.0012 | 0.60 | 0.075 | $1,999 | 59.2% | 238 | 31.8% | $2,364 |

## Key findings

1. mom=0.0012 is the sweet spot. Dominates top 7 results. Consistent with live run 006 which used v0.4 defaults near this value.

2. ask=0.60-0.65 required for strong results. Lower thresholds (0.45-0.55) reject too many entries. This contradicts v0.5's reduction to 0.55.

3. frac=0.10-0.125 optimal, balances returns vs drawdown.
   * frac=0.075: lower DD (31-32%) but ~40% less PnL
   * frac=0.15: marginal PnL gain with ~5% more DD

4. Two regimes visible:
   * High-frequency (mom=0.0012): 238-284 trades, 59-61% WR, best absolute PnL
   * Selective (mom=0.0016-0.0022): 56-150 trades, 63-73% WR, better risk-adjusted

5. Results align with live trading: top combo peak $4,741 vs run 006 live peak $9,678 (run 006 was 267h vs 189h here, and used both strategies).

## Comparison with sweep 002

Same parameters, only difference is PEAK_DD_PAUSE_PCT=1.0 vs 0.30.

| Metric | Sweep 002 (DD pause on) | Sweep 003 (DD pause off) |
|---|---|---|
| Best PnL | $1,282 | $4,070 |
| Best trades | 284 | 284 |
| Best WR | 73.2% | 60.9% |
| Matches live? | No | Yes |
