# Sweep rust-010, first sweep with real Polymarket oracle outcomes

Date: 2026-03-29
Status: VALID, the most accurate sweep to date

## Purpose

First parameter sweep using authoritative Polymarket resolution outcomes instead of Chainlink-derived settlements. All 9,238 markets in market-data.db now have `polymarket_outcome` populated from the Gamma API (via `verify-settlements`), and `build_data` overrides the Chainlink-derived `outcome` with the real Polymarket outcome. The backtester uses these outcomes directly via `resolve_window_with_outcome`, bypassing the Chainlink open/close price comparison entirely.

This eliminates the 5.4% settlement error rate that existed in all previous sweeps (001 through rust-009).

## Parameters

Same grid as rust-008 and rust-009.

- Data: `data/market-data.db` (runs 004-008, with polymarket_outcome from verify-settlements)
- Time range: 2026-02-15 to 2026-03-27 (~928h, ~39 days)
- Ticks: 11,074,264
- Balance: $200
- Swept: 6 x 5 x 5 = 150 combinations
- Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50`, `PEAK_DD_PAUSE_PCT=1.0`
- Runtime: 176s (~3 min)

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

- rust-009 (Chainlink): $54,145 PnL, 64.2% WR, 1060 trades, 29.7% DD
- rust-010 (Polymarket): $53,448 PnL, 65.7% WR, 849 trades, 33.2% DD

PnL dropped slightly ($697 or 1.3%). Win rate increased 1.5 percentage points (fewer trades but higher quality). Trade count dropped from 1060 to 849 (the Polymarket oracle flipped some window outcomes, causing different Kelly sizing cascades). DD increased from 29.7% to 33.2%.

## Best risk-adjusted combos (WR>60%, DD<35%)

1. mom=0.0014, ask=0.60, frac=0.125: $39,283, 64.3% WR, 33.3% DD
2. mom=0.0016, ask=0.55, frac=0.125: $28,539, 67.2% WR, 27.9% DD
3. mom=0.0018, ask=0.65, frac=0.125: $25,283, 67.7% WR, 30.3% DD
4. mom=0.0012, ask=0.50, frac=0.125: $17,235, 62.7% WR, 34.4% DD
5. mom=0.0018, ask=0.50, frac=0.125: $10,653, 65.2% WR, 33.9% DD

## Key findings

1. The strategy still shows edge with real Polymarket outcomes. PnL is slightly lower than Chainlink-derived sweeps (expected, since some "wins" were really losses).

2. Win rates are surprisingly similar or slightly higher with real outcomes. The Chainlink errors were roughly symmetric (didn't systematically inflate WR), but they did affect compounding differently (wrong wins followed by wrong losses create different Kelly sizing paths).

3. The optimal parameters shifted slightly. mom=0.0014 with ask=0.60 now appears stronger in the risk-adjusted ranking (was not in the rust-009 top 5).

4. frac=0.125 dominates the risk-adjusted ranking more strongly with real outcomes. Higher fraction benefits from the more accurate win/loss determination.

5. The run 008 parameters (mom=0.0012, ask=0.60, frac=0.05) remain solid but are no longer the undisputed best. They rank well for low DD (<35%) but other combos now show better absolute returns with similar risk.
