# Run 012 Baseline Parity Check

## Result

The exact-params replay still does not pass the parity gate after the replay-fidelity fixes. The full parameter sweep remains blocked.

Archived run 012:

- final balance: `$701.50`
- total PnL: `$602.33`
- trades: `180`
- wins/losses: `106/74`
- max drawdown: `50.04%`
- high water mark: `$1,404.22`
- last trade entry: `2026-04-23 02:58:36 UTC`

Original current-code replay on the derived run 012 DB:

- final balance: `$183.65`
- total PnL: `$84.48`
- trades: `136`
- wins/losses: `78/58`
- max drawdown: `36.68%`
- high water mark: `$290.03`
- last trade entry: `2026-04-23 02:58:37 UTC`

Replay after raw-event and window-open fixes:

- final balance: `$156.12`
- total PnL: `$56.95`
- trades: `142`
- wins/losses: `80/62`
- max drawdown: `42.8%`
- high water mark: `$272.77`
- last trade entry: `2026-04-22 19:58:36 UTC`

Material deltas after the fixes:

- final balance: `$-545.38`
- trades: `-38`
- max drawdown: `-7.2pp`

This fails the planned gate by a wide margin. The gate was `$25` final-balance difference, `10` trades, or `5pp` max-drawdown difference.

## What Was Verified

- The replay used only run 012 data.
- The archive DB itself was not modified.
- A derived replay DB was created at `data/experiments/run-012-forensics-001/run012_replay_data.db`.
- The derived DB adds `markets.open_price` and `markets.close_price`, computed from Chainlink feed events, because older archive markets did not expose those columns.
- The replay loaded `21,882,579` ticks and `159` windows.
- The replay used `BACKTEST_SETTLEMENT_MODE=observed_market_resolution`.
- The replay used risky pending-settlement mode: `0.0 / 0.25 / false`.
- The backtester now replays raw Binance aggTrade signed quantity, raw Binance depth summaries, raw CLOB top-of-book rows, raw event microsecond ordering, and native Binance window-open prices.

## Replay Attribution

Archived run:

- `latency-arb`: `110` trades, pnl `$804.71`, avg `$7.32`, worst `$-151.87`, best `$166.99`
- `calm-persistence`: `67` trades, pnl `$-140.61`, avg `$-2.10`, worst `$-46.39`, best `$71.58`
- `spread-capture`: `3` trades, pnl `$-61.77`, avg `$-20.59`, worst `$-34.06`, best `$-5.32`

Replay after raw-event and window-open fixes:

- `latency-arb`: `84` trades, pnl `$24.70`, avg `$0.29`, max size `56`
- `calm-persistence`: `58` trades, pnl `$32.25`, avg `$0.56`, max size `20`
- `spread-capture`: `0` trades

The divergence is not a small settlement-timing difference. It changes trade count, sizing path, strategy contribution, spread participation, and drawdown path.

## Root Cause

The remaining mismatch is not primarily code drift or missing env knobs. The run 012 archive was recorded with compact feed-event storage, and compact storage intentionally skipped Binance `bookTicker` rows.

Evidence:

- `feed_events` contains `0` Binance `bookTicker` rows.
- The deployed live runtime used Binance `bookTicker` updates in memory to update `SignalState.binance_book`.
- Archived filled signals have non-null `binanceBookImbalance` and `featureMode="raw_event_full"`, which proves those live-only book updates affected decisions.
- The fixed replay can only approximate Binance book imbalance from persisted depth summaries. It cannot reconstruct the exact missing `bookTicker` event stream or the exact decision events that it triggered.
- Archived run 012 has `267,210` signals. The fixed replay has only `5,021` signals because the missing `bookTicker` event stream removes a large part of the live decision cadence.
- The first material missing trade is market `1958564`: archive filled a `latency-arb DOWN` signal at `2026-04-13 13:50:37.080 UTC` with `upAsk=0.48`, `downAsk=0.53`, `quoteAgeMs=0`, and `binanceBookImbalance=0.2435`. The replay has no signal for that market and only rejection summaries.

The raw replay fix was still useful: it corrected the backtester for future full-fidelity runs and proved that run 012's archive lacks a required decision input. Run 012 can still support descriptive forensics and drawdown analysis, but it should not be used for trusted parameter optimization.

## Decision

Do not run the full sweep on run 012. A sweep on this replay would optimize a materially different event stream than the one that generated run 012.

Treat run 012 as descriptive-forensics-only. It was originally discussed as server run 013 before local renumbering. For future runs intended for exact replay and sweeps, record the decision-triggering Binance book stream or a compact derived equivalent that is sufficient to reconstruct `SignalState.binance_book` and decision cadence.
