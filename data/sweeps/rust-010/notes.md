# Sweep rust-010, first sweep with real Polymarket oracle outcomes

Date: 2026-03-29
Status: VALID, the most accurate sweep to date

## Purpose

First parameter sweep using authoritative Polymarket resolution outcomes instead of Chainlink-derived settlements. All 9,238 markets in market-data.db now have `polymarket_outcome` populated from the Gamma API (via `verify-settlements`), and `build_data` overrides the Chainlink-derived `outcome` with the real Polymarket outcome. The backtester uses these outcomes directly via `resolve_window_with_outcome`, bypassing the Chainlink open/close price comparison entirely.

This eliminates the 5.4% settlement error rate that existed in all previous sweeps (001 through rust-009).

## Parameters

Same grid as rust-008 and rust-009.

* Data: `data/market-data.db` (runs 004-008, with polymarket_outcome from verify-settlements)
* Time range: 2026-02-15 to 2026-03-27 (~928h, ~39 days)
* Ticks: 11,074,264
* Balance: $200
* Swept: 6 x 5 x 5 = 150 combinations
* Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50`, `PEAK_DD_PAUSE_PCT=1.0`
* Runtime: 176s (~3 min)

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-15 --end 2026-03-27 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-010/sweep.csv
```

## Run 008 params (mom=0.0012, ask=0.60, frac=0.05) comparison

* rust-009 (Chainlink): $54,145 PnL, 64.2% WR, 1060 trades, 29.7% DD
* rust-010 (Polymarket): $53,448 PnL, 65.7% WR, 849 trades, 33.2% DD

PnL dropped slightly ($697 or 1.3%). Win rate increased 1.5 percentage points (fewer trades but higher quality). Trade count dropped from 1060 to 849 (the Polymarket oracle flipped some window outcomes, causing different Kelly sizing cascades). DD increased from 29.7% to 33.2%.

## Best raw PnL combos

These are the top five rows by absolute PnL. Most of them carry materially higher drawdown than the balanced frontier.

1. mom=0.0008, ask=0.55, frac=0.125: $86,071, 59.6% WR, 2066 trades, 47.1% DD
2. mom=0.0008, ask=0.55, frac=0.03: $75,423, 61.3% WR, 1799 trades, 30.7% DD
3. mom=0.0008, ask=0.70, frac=0.05: $74,885, 64.7% WR, 2091 trades, 42.0% DD
4. mom=0.0010, ask=0.60, frac=0.075: $74,414, 66.3% WR, 1078 trades, 41.5% DD
5. mom=0.0010, ask=0.60, frac=0.05: $71,041, 61.0% WR, 1667 trades, 40.4% DD

## Best balanced combos (WR>60%, DD<35%)

These are the top five rows by PnL under the stated quality filter.

1. mom=0.0008, ask=0.55, frac=0.03: $75,423, 61.3% WR, 1799 trades, 30.7% DD
2. mom=0.0012, ask=0.55, frac=0.10: $61,723, 65.0% WR, 887 trades, 34.2% DD
3. mom=0.0012, ask=0.65, frac=0.075: $59,704, 63.6% WR, 1239 trades, 33.2% DD
4. mom=0.0012, ask=0.55, frac=0.05: $58,160, 65.2% WR, 876 trades, 30.1% DD
5. mom=0.0010, ask=0.70, frac=0.03: $56,721, 64.5% WR, 2054 trades, 29.2% DD

## Most reliable region

The clearest stable cluster is mom=0.0012 with ask between 0.55 and 0.60.

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0012` is the strongest default momentum. All 25 combinations are profitable. It has the highest average PnL in the sweep at $44,751, with 65.1% mean WR and 34.4% mean DD.
* `LATENCY_ARB_MAX_ASK=0.55` is the strongest default ask. It has the highest average PnL at $33,235. `0.60` is close behind. `0.50` is too restrictive, while `0.70` increases traffic and drawdown faster than it improves returns.
* `MAX_POSITION_FRACTION=0.05` is the best general sizing choice. It has the highest average PnL at $35,946, and all 30 combinations are profitable. `0.03` is the safest sizing choice, with 24.0% mean DD and 30 of 30 profitable rows.
* The specific region `mom=0.0012, ask=0.55` is unusually stable across size. Fractions 0.03 through 0.10 all land between $51,890 and $61,723 PnL, with 65.0% to 65.5% WR and 20.3% to 34.2% DD.

## Key findings

1. The strategy still shows clear edge with real Polymarket outcomes. The switch away from Chainlink-derived settlement reduced some inflated paths, but it did not collapse the signal.

2. The run 008 parameters remain solid, but `mom=0.0012, ask=0.55, frac=0.05` now looks like the cleaner default than `mom=0.0012, ask=0.60, frac=0.05`. It improves PnL from $53,448 to $58,160 while reducing DD from 33.2% to 30.1%.

3. Very low momentum settings can still produce the biggest absolute winners, but that region is fragile. For example, the `mom=0.0008, ask=0.55` slice ranges from -$60 to +$86,071 depending on fraction, and its average DD is 44.6%.

4. `mom=0.0014` with ask 0.55 to 0.60 remains a robust secondary region. It is profitable across all five fractions and less sensitive to size changes than the 0.0008 and 0.0010 peaks, but it does not beat the best `0.0012` rows on return.

5. `frac=0.125` does not dominate the risk-adjusted frontier in this sweep. Under the filter WR>60% and DD<35%, the counts are 23 rows for frac=0.03, 17 for frac=0.05, 13 for frac=0.075, 8 for frac=0.10, and 5 for frac=0.125.

## Reporting note

The CSV reports identical values for `pnl` and `pnl_net` on all 150 rows even though `total_fees` is non-zero. This note treats `pnl` as the operative metric exported by the current backtester.
