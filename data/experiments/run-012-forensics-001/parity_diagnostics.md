# Run 012 Parity Diagnostics

## Baseline Gate

The raw-event replay and native-window-open fixes were applied, then the exact risky baseline was rerun into `baseline_exact_after_replay_fix.db`.

Archive target:

* final balance: `$701.50`
* trades: `180`
* max drawdown: `50.04%`
* signals: `267,210`

Fixed replay:

* final balance: `$156.12`
* trades: `142`
* max drawdown: `42.8%`
* signals: `5,021`

The gate still fails. The difference is too large for parameter sweeps.

## First Divergences

The first trade already differs in size:

* archive: market `1952174`, `latency-arb DOWN`, entry `0.46`, size `14`, pnl `$-6.56`
* fixed replay: market `1952174`, `latency-arb DOWN`, entry `0.46`, size `20`, pnl `$-9.36`

The first material missing archive trade is market `1958564`:

* archive signal timestamp: `2026-04-13 13:50:37.080 UTC`
* archive trade fill timestamp: `2026-04-13 13:50:37.330 UTC`
* strategy and side: `latency-arb DOWN`
* archive entry: `0.54`
* archive size: `10`
* archive feature state: `featureMode=raw_event_full`, `upAsk=0.48`, `downAsk=0.53`, `quoteAgeMs=0`, `bookStalenessMs=0`, `binanceBookImbalance=0.2435`, `polymarketQuoteChurnPerS=140`
* fixed replay: no signal for `1958564`; rejection summaries exist, but no selected candidate

That missing early win changes the sizing path. Later differences compound through balance-dependent sizing, cooldown, sleeve pressure, drawdown state, and pending-settlement reserves.

## Feature Reconstruction

The fixed replay now produces `raw_event_full` feature snapshots from persisted raw events:

* Binance aggTrade signed quantity is replayed.
* Binance depth summaries are replayed.
* CLOB top-of-book events are replayed one event at a time.
* Raw event microsecond ordering is preserved without same-source collapse.
* Window open now prefers native Binance prices over Chainlink-derived `markets.open_price`.

That closes the original `legacy_core` problem, but it exposes a deeper data-capture limit.

## Remaining Root Cause

Run 012 was captured with compact feed-event storage. Compact storage intentionally skipped Binance `bookTicker` persistence.

Evidence:

* `feed_events` has `0` rows with `source='binance'` and `event_type='bookTicker'`.
* The live runtime did process Binance `bookTicker` messages in memory and used them to update `SignalState.binance_book`.
* Archived filled signal metrics contain non-null `binanceBookImbalance`, proving live decisions used book-ticker state that is absent from the archived feed-events table.
* The fixed replay approximates book state from Binance depth summaries, but this is not the same event stream, not the same timing, and not the same book imbalance.
* The fixed replay has only `5,021` signal rows versus `267,210` in the archive. The missing decision-triggering book stream is the dominant remaining parity break.

## Decision

Run 012 should not be used for trusted parameter sweeps. It remains useful for descriptive forensics: drawdown chain, realized trade attribution, halt behavior, operational health, and broad qualitative strategy review. It was originally discussed as server run 013 before local renumbering.

Future sweep-grade live-paper runs need one of these capture modes:

* Persist Binance `bookTicker` rows in full-fidelity mode for research runs.
* Persist a compact, deduplicated book-ticker-derived state stream that is sufficient to reconstruct `SignalState.binance_book` and decision cadence.
* Persist signal-anchor snapshots for every strategy evaluation if the goal is exact replay of the deployed decision stream rather than raw-feed replay.
