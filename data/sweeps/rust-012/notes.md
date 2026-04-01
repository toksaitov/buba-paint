# Sweep rust-012, first run on the v2 feature engine and rebuilt merged DB

Date: 2026-04-01
Status: VALID, rebuilt from runs `004` through `009`

## Purpose

This is the first full sweep after landing the raw-event-compatible schema, live-paper instrumentation, and shared v2 signal feature engine.

The important caveat is that the historical source data is still old-style only. `rust-012` therefore answers a narrower question than the full raw-event plan:

- the code path is now v2
- the replay source is still entirely `legacy_snapshot`
- the feature mode exercised by this sweep is effectively `legacy_core`
- no future raw-event run has populated `signal_metrics` or `feed_health_events` yet

So this sweep measures whether the new feature/scoring model improves the frontier before any true raw-event live run exists.

## Parameters

- Data: `data/market-data.db`
- Time range: 2026-02-15 19:10 through 2026-03-31 19:35
- Runs included: `004`, `005`, `006`, `007`, `008`, `009`
- Markets: `9,944`
- Historical trades: `1,056`
- Feed events: `11,918,164`
- Replay batches per run: `3,002,577`
- Replay fidelity: all rows are `legacy_snapshot` only (`raw_event_batches=0` on every row)
- Source telemetry tables in the merged DB: `signal_metrics=0`, `feed_health_events=0`
- Sweep mode: new v2 strategies running on legacy-core replay inputs
- Balance: `$200`
- Swept: `6 x 5 x 5 x 5 = 750` combinations
- Fixed: `TAKER_FEE_RATE=0.072`, `TAKER_FEE_EXPONENT=1`, `SIM_ORDER_LATENCY_MS=250`
- Runtime: about `11.7` minutes on `14` cores

## Command

```bash
cargo run --release -p buba-paint -- sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/rust-012/sweep.csv
```

## Top raw PnL combos

These are the top five rows by `pnl_net`.

1. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.965`: `$37,398`, `71.7%` WR, `435` trades, `34.0%` DD, `53.2%` fill rate, `75` legging events
2. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.970`: `$37,309`, `71.9%` WR, `437` trades, `34.0%` DD, `52.0%` fill rate, `77` legging events
3. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.965`: `$35,431`, `71.8%` WR, `432` trades, `33.6%` DD, `52.4%` fill rate, `72` legging events
4. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.970`: `$35,398`, `71.9%` WR, `434` trades, `33.6%` DD, `51.3%` fill rate, `74` legging events
5. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.975`: `$34,803`, `68.9%` WR, `463` trades, `36.1%` DD, `49.5%` fill rate, `109` legging events

## Best balanced combos (WR > 60%, DD < 35%)

1. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.965`: `$37,398`, `71.7%` WR, `435` trades, `34.0%` DD
2. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.970`: `$37,309`, `71.9%` WR, `437` trades, `34.0%` DD
3. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.965`: `$35,431`, `71.8%` WR, `432` trades, `33.6%` DD
4. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.970`: `$35,398`, `71.9%` WR, `434` trades, `33.6%` DD
5. mom=`0.0008`, ask=`0.60`, frac=`0.075`, spread=`0.975`: `$33,099`, `69.9%` WR, `465` trades, `30.3%` DD

Strict drawdown candidates:

- DD < `20%`: mom=`0.0008`, ask=`0.65`, frac=`0.05`, spread=`0.970` -> `$25,335`, `74.2%` WR, `454` trades, `19.2%` DD
- DD < `25%`: mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970` -> `$28,043`, `73.3%` WR, `412` trades, `20.7%` DD
- DD < `30%`: mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970` -> `$28,043`, `73.3%` WR, `412` trades, `20.7%` DD

## Fidelity and feature-mode mix

This run still cannot measure the raw-event frontier directly.

- every sweep row has `raw_event_batches=0`
- every sweep row has `legacy_snapshot_batches=3002577`
- the rebuilt merged DB contains no source-side `signal_metrics` or `feed_health_events` rows yet
- historical logged signals in the merged DB remain old-run `v1/legacy_core`
- the sweep itself uses the new v2 strategies and feature engine, but only against legacy-core replay inputs

So `rust-012` is the first answer to: "does the upgraded signal model help even before true raw-event capture exists?" The answer is yes, but it is still bounded by 1 Hz history.

## Comparison to refreshed rust-011

This is not an apples-to-apples simulator change only. `rust-012` keeps the same rebuilt historical dataset as the refreshed `rust-011`, but replaces the old strategy logic with the new v2 feature/scoring path on top of that dataset.

Net effect:

- mean `pnl_net` rose from about `$1,388` to about `$2,991`
- positive rows rose from `295/750` to `497/750`
- `473` rows improved and `277` worsened
- the best raw-return row changed from `frac=0.05 / spread=0.970` to a more aggressive `frac=0.125 / spread=0.965`
- the old `ask=0.70` safe frontier largely disappeared

Anchor comparisons:

- Previous best row: `mom=0.0008, ask=0.60, frac=0.05, spread=0.970`
  - refreshed `rust-011`: `$43,000`, `61.6%` WR, `1571` trades, `25.3%` DD
  - `rust-012`: `$28,043`, `73.3%` WR, `412` trades, `20.7%` DD
  - change: `-$14,957`, with a much tighter and higher-quality trade set

- New best row: `mom=0.0008, ask=0.60, frac=0.125, spread=0.965`
  - refreshed `rust-011`: `-$100`, `34.4%` WR, `32` trades, `50.0%` DD
  - `rust-012`: `$37,398`, `71.7%` WR, `435` trades, `34.0%` DD
  - change: `+$37,498`, showing how strongly the v2 model re-ranked sizing and spread regions

- Old conservative winner: `mom=0.0008, ask=0.70, frac=0.05, spread=0.965`
  - refreshed `rust-011`: `$26,823`, `66.2%` WR, `1608` trades, `19.0%` DD
  - `rust-012`: `-$51`, `48.5%` WR, `33` trades, `28.3%` DD
  - change: `-$26,874`, which effectively removes this row from consideration

- Moderate-momentum candidate: `mom=0.0010, ask=0.60, frac=0.05, spread=0.965`
  - refreshed `rust-011`: `$20,732`, `68.0%` WR, `500` trades, `20.2%` DD
  - `rust-012`: `$9,153`, `74.2%` WR, `252` trades, `16.9%` DD
  - change: `-$11,579`, again trading raw return for stricter signal quality

So the new model broadens the profitable surface and raises average quality, but it does so by concentrating the frontier in a narrower low-momentum regime and discarding large parts of the old moderate/safe frontier.

## Most important findings

1. The v2 feature engine materially improves the sweep average even on legacy-only data.
   - positive rows increased from `295` to `497`
   - mean `pnl_net` more than doubled to about `$2,991`

2. `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008` now dominates the search space.
   - mean `pnl_net` is about `$10,857`
   - all higher thresholds degrade sharply, especially `0.0014+`

3. `LATENCY_ARB_MAX_ASK=0.60` remains the strongest ask cap by a wide margin.
   - mean `pnl_net` is about `$6,733`
   - `0.65` is still viable as a stricter-DD region
   - `0.70` no longer carries the old safety frontier

4. `SPREAD_CAPTURE_THRESHOLD=0.970` is now the best mean spread setting, with `0.965` close behind.
   - `0.975+` still degrades materially

5. The size regime changed.
   - mean `pnl_net` is highest at `frac=0.05`
   - the raw top rows now use `frac=0.10` and `0.125`
   - the quality frontier still favors `0.05` and `0.075`
   - `0.03` remains clean, but it is no longer the only sane size region

6. Fill realism still dominates the practical ceiling.
   - mean fill rate across the sweep is about `25.2%`
   - mean partial-fill rate is about `9.5%`
   - mean no-fill count is about `475.6` per run
   - mean spread legging count is about `55.4` per run

Even the top row still carries `75` spread legging events. This remains a major live caveat regardless of the improved backtest surface.

## Current shortlist

If optimizing for raw return under the new v2 legacy-core model:

- `mom=0.0008, ask=0.60, frac=0.125, spread=0.965`
- `mom=0.0008, ask=0.60, frac=0.125, spread=0.970`

If optimizing for a tighter drawdown profile:

- `mom=0.0008, ask=0.65, frac=0.05, spread=0.970`
- `mom=0.0008, ask=0.60, frac=0.05, spread=0.970`
- `mom=0.0010, ask=0.60, frac=0.075, spread=0.970`

If optimizing for a moderate size with strong quality:

- `mom=0.0008, ask=0.60, frac=0.05, spread=0.970`
- `mom=0.0008, ask=0.60, frac=0.05, spread=0.965`
- `mom=0.0010, ask=0.60, frac=0.05, spread=0.965`

## Reporting note

The CSV still reports identical values for `pnl` and `pnl_net` on all `750` rows even though `total_fees` is non-zero. `gross_pnl` is distinct, so this note again treats `pnl_net` as the operative exported metric.

Also note that this sweep does not yet validate the raw-event capture work itself. The raw-event schema, live-paper instrumentation, and operator probe are implemented, but a future live-paper run needs to populate the new tables before `rust-013+` can evaluate a true `raw_event_full` frontier.
