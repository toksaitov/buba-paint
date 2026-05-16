# Sweep run-018-001, exact-run frontier on the pulled `run-018` snapshot

This sweep answers one narrow question:

* if we freeze the live `run-018` data pulled from `buba-paint`
* and replay that run locally
* how do the current live params rank against a full local frontier on that exact run?

It also exposed an important caveat:

* the backtester and live bot do **not** currently model capital release on the same timeline for this run
* so the frontier is useful for relative shape, but it is **not** yet trustworthy as an apples-to-apples live predictor

## Source Data

Pulled without stopping the bot:

* live DB + WAL + SHM copied into [runs/010](/Users/toksaitov/Desktop/buba-paint/runs/010)
* analysis snapshot derived locally at `/tmp/run-018-analysis.db`
* replay-compatible copy derived locally at `/tmp/run-018-replay-data.db`

Frozen replay interval:

* start: `2026-04-04T20:15`
* end: `2026-04-08T17:25`

The replay-compatible copy was created by adding `open_price` / `close_price` to the pulled run DB and backfilling them from the recorded Chainlink ticks so the standard backtest window loader could operate on the exact run snapshot.

## Command

```bash
./target/release/buba-paint sweep \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 --end 2026-04-08T17:25 \
  --balance 200 \
  --output data/sweeps/run-018-001/sweep.csv \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep LATENCY_ARB_MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set LATENCY_ARB_ENABLED=1 \
  --set SPREAD_CAPTURE_ENABLED=1 \
  --set CALM_PERSISTENCE_ENABLED=1 \
  --set REGIME_DETECTION_ENABLED=1 \
  --set TREND_FILTER_PER_STRATEGY=1 \
  --set MAX_POSITION_FRACTION=0.05 \
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
  --set CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --set MIN_WINDOW_TIME_MS=90000
```

Runtime:

* `750` combinations
* `~92.7 min`
* `10,141,915` raw-event batches
* `0` legacy-snapshot batches

## Actual Live Result On This Run

The frozen live snapshot shows:

* total realized `pnl_net`: about `-$9.05`
* total settled trades: `44`
* by strategy:
  * `latency-arb`: `38` trades, about `-$17.17`
  * `calm-persistence`: `6` trades, about `+$8.12`
  * `spread-capture`: `0`

Latency-arb was the weak leg in the real live run.

## Current Live Row In Replay

Current deployed latency/spread core row:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.65`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.05`
* `SPREAD_CAPTURE_THRESHOLD=0.970`

Exact-run replay result for that row:

* `pnl_net = $558.11`
* `trades = 170`
* `win_rate = 68.8%`
* `max_dd = 21.7%`
* `signals = 19,472`
* regime fills:
  * `dislocation_filled = 120`
  * `calm_filled = 48`

Ranking:

* raw rank: `101 / 750`
* rank with `max_dd <= 20%`: `16 / 490`
* rank with `max_dd <= 25%`: `51 / 595`
* rank with `max_dd <= 30%`: `81 / 680`

So on the replay frontier, the current row is:

* not top-tier raw PnL
* but still a decent conservative row

If replay were the only truth, this would not look like a broken parameter choice.

## Why Replay Does Not Match Live

This is the main finding.

The exact-run replay does **not** resemble the actual live result closely enough.

Replay of the same run and same current config produced:

* `+$558.11`
* `170` trades

Live on the same frozen run produced:

* `-$9.05`
* `44` trades

That gap is too large to explain by ordinary replay noise.

The code path explains it:

* backtest settles trades immediately at window close in [runner.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/backtest/runner.rs#L253)
* live holds trades open until authoritative Polymarket resolution in [live.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/live.rs#L962) and [position_manager.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/position_manager.rs#L281)
* sleeves and available capital are enforced off current reserved capital in [bankroll.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/bankroll.rs#L658)

On this live run:

* latency-arb had `55` `strategy_sleeve_exhausted` signal rejections
* replay for the same row had `0` capital-blocked events
* `55 / 55` of those live sleeve rejections happened while another latency-arb trade was still unresolved
* `53 / 55` also overlapped with an open calm trade

Settlement lag was not small:

* average latency-arb settlement lag after window end: about `5,286s`
* max latency-arb settlement lag: about `118,874s`

So replay is materially more optimistic because it frees capital at close, while live often keeps capital tied up for much longer.

That is the dominant explanation for why live generated far fewer trades and why latency-arb underperformed much more than the exact-run replay suggests.

## Frontier Shape

The sweep shape is still informative.

### Top raw rows

The top raw rows are all:

* `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
* `LATENCY_ARB_MAX_ASK=0.60`
* `LATENCY_ARB_MAX_POSITION_FRACTION=0.10`
* `SPREAD_CAPTURE_THRESHOLD=0.965-0.985`

Metrics:

* `pnl_net = $1,194.15`
* `trades = 110`
* `win_rate = 69.1%`
* `max_dd = 32.9%`

### Best DD-bounded rows

Best row with `max_dd <= 20%`:

* `0.0012 / 0.70 / 0.10 / 0.965`
* `pnl_net = $714.01`
* `trades = 110`
* `win_rate = 78.2%`
* `max_dd = 19.6%`

Best row with `max_dd <= 25%`:

* `0.0008 / 0.60 / 0.075 / 0.965`
* `pnl_net = $984.32`
* `trades = 115`
* `win_rate = 69.6%`
* `max_dd = 24.0%`

### Parameter means

By mean `pnl_net` over the exact-run frontier:

* momentum:
  * `0.0008` is best by a wide margin
* ask:
  * `0.70` has the highest mean raw PnL
  * `0.65` is close behind
  * `0.60` produces the cleanest stronger rows under DD limits
* latency sleeve:
  * higher sleeves improve mean raw PnL
  * but drawdown rises steadily
* spread threshold:
  * effectively irrelevant on this run
  * `0.965` through `0.985` are functionally identical here

## Interpretation

Two things can be true at once:

1. Live latency-arb really did underperform on `run-018`.
2. The exact-run sweep still does **not** validate the live result tightly enough to use as a deployment oracle.

The run-specific frontier says:

* the current row was not the best possible replay row on this run
* but it was still reasonably competitive under drawdown constraints

The live DB says:

* the actual live outcome was much worse
* mostly because live capital remained tied up across delayed authoritative settlement, creating many sleeve-exhausted missed opportunities that replay did not reproduce

So the main issue exposed by this exercise is not “latency-arb parameters are obviously wrong.”

The main issue is:

* live and backtest still diverge materially on capital-availability timing

## Recommendation

Do **not** use `run-018-001` as authority for changing live params yet.

Recommended next step:

1. make backtest capital release match live authoritative settlement timing for raw-event runs
2. rerun this exact-run sweep after that fidelity fix
3. only then reconsider whether `LATENCY_ARB_MAX_ASK` should move from `0.65` toward `0.60`

If forced to choose one exact-run row from the current imperfect frontier, the most defensible upgrade target is:

* `0.0008 / 0.60 / 0.075 / 0.965`

Why:

* much stronger than the current replay row
* still below `25%` DD
* same strong `0.0008` momentum
* spread threshold does not materially matter here anyway

But that would still be premature to deploy before the backtest/live settlement-capital mismatch is fixed.
