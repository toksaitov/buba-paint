# Sweep rust-014, full rerun after execution and spread-capture corrections

`rust-014` is the first full 750-row rerun after the recent live-driven fixes:

* corrected fillability and freshness handling
* corrected single-order submit-time minimum sizing
* synchronized spread-capture book handling
* explicit spread queue and rejection diagnostics
* optional spread-only sizing knob added to config

This sweep answers the narrow question that `spread-knob-001` did not: did the full historical frontier move after those simulator/execution-path fixes?

It did. Materially.

## Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8 \
  --set SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25 \
  --output data/sweeps/rust-014/sweep.csv
```

Important detail: `SPREAD_CAPTURE_MAX_POSITION_FRACTION` was intentionally left unset here. That means spread sizing still falls back to the swept `MAX_POSITION_FRACTION`, which keeps this rerun comparable to `rust-013`. The separate spread-cap question was already tested in `spread-knob-001`.

## Result

`rust-014` is not a reproducibility confirmation. The frontier moved.

* Changed rows versus `rust-013`: `750 / 750`
* Rows with higher `pnl_net` than `rust-013`: `725 / 750`
* Rows with lower `pnl_net` than `rust-013`: `25 / 750`
* Positive rows: `745 / 750`
* Mean `pnl_net`: `$7,787.58`
* Runtime: `705.9s` for all `750` combinations

The dominant shape of the change is:

* the previously recommended balanced rows improved sharply
* fill rates jumped
* spread legging counts collapsed on the strong rows
* a small cluster of high-risk `ask=0.55` rows got much worse, which is actually believable after the spread corrections

## Top raw PnL rows

The raw-return frontier is now dominated by `ask=0.70`, not `ask=0.60`.

1. mom=`0.0008`, ask=`0.70`, frac=`0.125`, spread=`0.965`: `$41,007`, `77.0%` WR, `538` trades, `41.2%` DD, `70.7%` fill rate, `8` legging events
2. mom=`0.0008`, ask=`0.70`, frac=`0.125`, spread=`0.970`: `$41,007`, `77.0%` WR, `538` trades, `41.2%` DD, `70.7%` fill rate, `8` legging events
3. mom=`0.0008`, ask=`0.70`, frac=`0.125`, spread=`0.975`: `$41,007`, `77.0%` WR, `538` trades, `41.2%` DD, `70.7%` fill rate, `8` legging events
4. mom=`0.0008`, ask=`0.70`, frac=`0.125`, spread=`0.980`: `$41,007`, `77.0%` WR, `538` trades, `41.2%` DD, `70.7%` fill rate, `8` legging events
5. mom=`0.0008`, ask=`0.70`, frac=`0.125`, spread=`0.985`: `$40,727`, `76.8%` WR, `539` trades, `41.2%` DD, `70.5%` fill rate, `9` legging events

These are strong, but still too aggressive for a conservative paper-live run.

## Balanced and strict-DD shortlist

The balanced winner moved upward in absolute quality but not in overall character.

* Best strong row under `20%` DD: mom=`0.0008`, ask=`0.65`, frac=`0.05`, spread=`0.965` -> `$32,067`, `77.7%` WR, `448` trades, `19.1%` DD, `68.5%` fill rate, `8` legging events
* The same row at spread `0.970`, `0.975`, and `0.980` is identical on all meaningful metrics
* Slightly more aggressive alternative: mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970` -> `$31,164`, `77.0%` WR, `374` trades, `18.7%` DD
* Higher-activity but rougher alternative: mom=`0.0008`, ask=`0.70`, frac=`0.05`, spread=`0.970` -> `$31,130`, `76.8%` WR, `538` trades, `25.3%` DD
* Best row under `30%` DD: mom=`0.0008`, ask=`0.65`, frac=`0.075`, spread=`0.965` -> `$37,899`, `77.7%` WR, `448` trades, `27.9%` DD

The conservative recommendation therefore stays in the same neighborhood:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_THRESHOLD=0.970`

I would keep `0.970` live even though `0.965-0.980` are identical here, because:

* it matches the current live configuration
* it avoids unnecessary churn
* `0.985` is the first threshold where the row actually starts to change

## Comparison to rust-013

The important change is not just higher raw PnL. The structure of the good rows got cleaner.

For the current recommended row:

* `0.0008 / 0.65 / 0.05 / 0.970`
  * `rust-013`: `$25,335`, `74.2%` WR, `454` trades, `19.2%` DD, `43.6%` fill rate, `55` legging events
  * `rust-014`: `$32,067`, `77.7%` WR, `448` trades, `19.1%` DD, `68.5%` fill rate, `8` legging events

That is a real upgrade:

* `+$6.7k` net PnL
* better win rate
* essentially unchanged drawdown
* much higher fill rate
* dramatically lower legging count

Other notable moves:

* `0.0008 / 0.60 / 0.05 / 0.970`: `$28,043` -> `$31,164`
* `0.0008 / 0.70 / 0.05 / 0.970`: `$14,580` -> `$31,130`
* `0.0008 / 0.70 / 0.125 / 0.965`: `-$102` -> `$41,007`

The biggest negative moves were concentrated in the old `ask=0.55`, `frac=0.125` cluster:

* `0.0008 / 0.55 / 0.125 / 0.965`: `$29,133` -> `-$107`
* `0.0008 / 0.55 / 0.125 / 0.970`: `$28,534` -> `-$107`

That pattern is believable. Those rows looked overly spread-dependent before. After the spread synchronization and execution fixes, they no longer get unrealistic help from sloppy spread behavior.

## Interpretation

The recent fixes changed the simulator in a direction that looks more realistic, not less:

* strong latency-arb rows improved because the execution path is no longer poisoned by stale freshness handling, missing-size overwrites, and broken submit-time minimum sizing
* bad spread-heavy rows lost their unrealistic edge because the strategy now requires synchronized books and uses stricter execution semantics

The biggest practical signal in `rust-014` is:

* the good rows are better
* the pathological rows are less flattering
* spread threshold barely matters on the balanced frontier until `0.985`

That is a healthier result than `rust-013`.

## Fidelity caveat

This is still the same legacy-only historical universe:

* `raw_event_batches=0`
* `legacy_snapshot_batches=3,002,577`

So `rust-014` is not yet the last word on future raw-event live-paper replay. But it is the correct current answer for the merged `004-009` historical dataset under the fixed simulator.

## Deployment recommendation

For the current live-paper program, the recommendation is:

* keep the conservative `run-017` style row
* do not revive the aggressive `run-016` spread activation experiment based on this sweep
* if you want the next step up after `run-017`, prefer `0.0008 / 0.60 / 0.05 / 0.970` before jumping to the higher-DD `ask=0.70` or `frac=0.075+` rows

So the operational recommendation remains:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_THRESHOLD=0.970`
* `SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8`
* `SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25`
* `TAKER_FEE_RATE=0.072`
* `TAKER_FEE_EXPONENT=1`
* `SIM_ORDER_LATENCY_MS=250`

That is still the best clean paper-live choice after the full rerun.
