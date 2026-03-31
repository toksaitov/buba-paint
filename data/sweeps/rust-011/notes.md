# Sweep rust-011, refreshed with run 009 and the rebuilt merged DB

Date: 2026-04-01
Status: VALID, refreshed after importing and upgrading `runs/009`

## Purpose

This reruns `rust-011` after pulling `runs/009` from the old remote bot, upgrading that DB to the modern historical schema, verifying its settlements against Gamma, and rebuilding `data/market-data.db` from runs `004` through `009`.

This refresh materially changes the dataset versus the previous `rust-011`:

- added run `009` (`v0.8.1`) to the merged historical universe
- upgraded `runs/009` in place with `feed_events`, `history_upgrades`, signal execution metadata, and trade execution-audit fields
- verified settlements on `runs/009`, which filled `polymarket_outcome` for `705/706` markets
- the merged DB overrode `31` `run 009` Chainlink-derived outcomes with authoritative Polymarket outcomes

The execution model is unchanged from the earlier live-like `rust-011`. This rerun answers a narrower question: how much does adding the real `run 009` history change the frontier?

## Parameters

- Data: `data/market-data.db`
- Time range: 2026-02-15 19:10 through 2026-03-31 19:40
- Runs included: `004`, `005`, `006`, `007`, `008`, `009`
- Markets: `9,944`
- Historical trades: `1,056`
- Feed events: `11,918,164`
- Replay batches per run: `3,002,577`
- Replay fidelity: all rows are `legacy_snapshot` only (`raw_event_batches=0` on every row)
- Balance: `$200`
- Swept: `6 x 5 x 5 x 5 = 750` combinations
- Fixed: `TAKER_FEE_RATE=0.072`, `TAKER_FEE_EXPONENT=1`, `SIM_ORDER_LATENCY_MS=250`
- Runtime: about `11.7` minutes on `14` cores

## Command

```bash
cargo run --release -q -p buba-paint -- sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:40 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/rust-011/sweep.csv
```

## Top raw PnL combos

These are the top five rows by `pnl_net`.

1. mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970`: `$43,000`, `61.6%` WR, `1571` trades, `25.3%` DD, `40.3%` fill rate, `73` legging events
2. mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.965`: `$35,465`, `62.4%` WR, `1427` trades, `23.2%` DD, `35.6%` fill rate, `40` legging events
3. mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.970`: `$35,434`, `61.6%` WR, `1582` trades, `27.4%` DD, `39.8%` fill rate, `43` legging events
4. mom=`0.0012`, ask=`0.60`, frac=`0.125`, spread=`0.970`: `$35,423`, `67.5%` WR, `511` trades, `36.3%` DD, `25.7%` fill rate, `86` legging events
5. mom=`0.0012`, ask=`0.60`, frac=`0.10`, spread=`0.965`: `$34,922`, `64.6%` WR, `697` trades, `36.8%` DD, `36.2%` fill rate, `77` legging events

## Best balanced combos (WR > 60%, DD < 35%)

These are the top five rows under the stated quality filter.

1. mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970`: `$43,000`, `61.6%` WR, `1571` trades, `25.3%` DD
2. mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.965`: `$35,465`, `62.4%` WR, `1427` trades, `23.2%` DD
3. mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.970`: `$35,434`, `61.6%` WR, `1582` trades, `27.4%` DD
4. mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.975`: `$34,291`, `62.9%` WR, `1424` trades, `22.8%` DD
5. mom=`0.0008`, ask=`0.65`, frac=`0.05`, spread=`0.965`: `$31,798`, `63.4%` WR, `1498` trades, `26.9%` DD

Strict drawdown candidates:

- DD < `20%`: mom=`0.0008`, ask=`0.70`, frac=`0.05`, spread=`0.965` -> `$26,823`, `66.2%` WR, `1608` trades, `19.0%` DD
- DD < `25%`: mom=`0.0008`, ask=`0.60`, frac=`0.03`, spread=`0.965` -> `$35,465`, `62.4%` WR, `1427` trades, `23.2%` DD
- DD < `30%`: mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970` -> `$43,000`, `61.6%` WR, `1571` trades, `25.3%` DD

## Comparison to the previous rust-011

This refresh is mildly harsher overall, but it does not overturn the main frontier.

- previous `rust-011` used `11,073,066` feed events and `2,791,302` replay batches; refreshed `rust-011` uses `11,918,164` feed events and `3,002,577` replay batches
- `run 009` added `706` markets, `845,098` legacy replay events, and `25` historical trades
- the merged build overrode `31` `run 009` outcomes with authoritative Polymarket outcomes after settlement verification
- mean `pnl_net` fell from about `$1,624` to about `$1,388`
- positive rows fell from `297/750` to `295/750`
- `219` rows worsened, `12` improved, and `519` were unchanged

Anchor comparisons:

- Old best row, still the new best row: `mom=0.0008, ask=0.60, frac=0.05, spread=0.970`
  - previous `rust-011`: `$45,849`, `63.1%` WR, `1476` trades, `25.3%` DD
  - refreshed `rust-011`: `$43,000`, `61.6%` WR, `1571` trades, `25.3%` DD
  - change: `-$2,849`, or about `-6.2%`

- Conservative winner: `mom=0.0008, ask=0.70, frac=0.05, spread=0.965`
  - previous `rust-011`: `$29,037`, `67.7%` WR, `1457` trades, `19.0%` DD
  - refreshed `rust-011`: `$26,823`, `66.2%` WR, `1608` trades, `19.0%` DD
  - change: `-$2,214`, or about `-7.6%`

- Moderate-momentum candidate: `mom=0.0010, ask=0.60, frac=0.05, spread=0.965`
  - previous `rust-011`: `$25,342`, `73.2%` WR, `444` trades, `20.2%` DD
  - refreshed `rust-011`: `$20,732`, `68.0%` WR, `500` trades, `20.2%` DD
  - change: `-$4,610`, or about `-18.2%`

So the added `run 009` data trims the edge, but mostly by reducing return and win rate, not by creating a new drawdown regime.

## Most important findings

1. The edge survives the `run 009` refresh. The best raw-return row and the best strict-drawdown row are still the same parameter families as before.

2. `LATENCY_ARB_MAX_ASK=0.60` remains the strongest default ask by mean return. Its mean `pnl_net` is about `$3,321`, still clearly ahead of the other ask caps. `0.70` remains the strongest strict-DD region.

3. `SPREAD_CAPTURE_THRESHOLD=0.970` remains the best spread setting. Its mean `pnl_net` is about `$2,909`, slightly better than `0.965` at about `$2,587`. Thresholds `0.975` and above still degrade sharply.

4. Sizing is still harsh. `MAX_POSITION_FRACTION=0.05` has the highest mean `pnl_net` at about `$3,007`, but the safer/risk-adjusted frontier is still dominated by `0.03`.
   - Positive rows by fraction: `0.03=132`, `0.05=94`, `0.075=44`, `0.10=16`, `0.125=9`
   - Rows with `WR>60%` and `DD<35%`: `0.03=100`, `0.05=48`, `0.075=5`, `0.10=1`, `0.125=0`

5. Momentum still splits upside and robustness.
   - `mom=0.0008` has the highest mean `pnl_net` at about `$3,039`, but only `42/125` of its rows are positive
   - `mom=0.0012` still has the broadest positive coverage at `61/125` rows
   - `mom=0.0010` remains a reasonable moderate region, but the refresh weakened it more than the `0.0008 / 0.60-0.70` region

6. Fill realism is still the main constraint.
   - Mean fill rate across the sweep is only `13.6%`
   - Mean partial-fill rate is `8.4%`
   - Mean no-fill count is `1504.5` per run
   - Mean spread legging count is `44.6` per run

The top row still carries `73` spread legging events. This remains a meaningful live-risk caveat even after the `run 009` refresh.

## Current shortlist

If optimizing for raw return under the refreshed dataset:

- `mom=0.0008, ask=0.60, frac=0.05, spread=0.970`

If optimizing for a stricter drawdown profile:

- `mom=0.0008, ask=0.70, frac=0.05, spread=0.965`
- `mom=0.0008, ask=0.60, frac=0.03, spread=0.965`

If optimizing for a more moderate momentum region that still looks deployable:

- `mom=0.0010, ask=0.60, frac=0.05, spread=0.965`
- `mom=0.0012, ask=0.60, frac=0.05, spread=0.965`
- `mom=0.0012, ask=0.60, frac=0.05, spread=0.970`

## Reporting note

The CSV again reports identical values for `pnl` and `pnl_net` on all `750` rows even though `total_fees` is non-zero. `gross_pnl` is distinct, so this note treats `pnl_net` as the operative exported metric.

Also note that every row shows `raw_event_batches=0` and `legacy_snapshot_batches=3002577`. This refreshed `rust-011` is better than the earlier one because it includes the real `run 009` history, but it is still bounded by `1 Hz` legacy replay fidelity.
