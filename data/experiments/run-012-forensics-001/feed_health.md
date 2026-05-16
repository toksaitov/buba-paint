# Run 012 Feed and Runtime Health

Feed events covered `2026-04-12 15:34:53 UTC` to `2026-04-24 18:34:54 UTC` with `21,882,579` raw rows.

## Feed Event Mix

* `binance` / `aggTrade`: `8,832,899` rows from `2026-04-12 15:34:54 UTC` to `2026-04-24 18:34:53 UTC`
* `clob_up` / `best_bid_ask`: `4,327,321` rows from `2026-04-12 15:34:55 UTC` to `2026-04-24 18:34:53 UTC`
* `clob_down` / `best_bid_ask`: `4,325,128` rows from `2026-04-12 15:34:55 UTC` to `2026-04-24 18:34:53 UTC`
* `binance` / `depth`: `3,271,777` rows from `2026-04-12 15:34:54 UTC` to `2026-04-24 18:34:54 UTC`
* `chainlink` / `chainlink_price`: `1,035,298` rows from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:34:54 UTC`
* `clob` / `new_market`: `80,368` rows from `2026-04-12 15:38:02 UTC` to `2026-04-24 18:34:31 UTC`
* `clob_down` / `book`: `4,462` rows from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:30:00 UTC`
* `clob_up` / `book`: `4,461` rows from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:30:00 UTC`
* `clob` / `tick_size_change`: `796` rows from `2026-04-12 20:43:59 UTC` to `2026-04-24 18:19:48 UTC`
* `clob` / `market_resolved`: `69` rows from `2026-04-21 06:12:14 UTC` to `2026-04-24 18:27:15 UTC`

## Feed Health Events

* `clob` / `connected`: `3,906` events from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:30:00 UTC`
* `binance` / `connected`: `986` events from `2026-04-12 15:34:54 UTC` to `2026-04-24 18:26:37 UTC`
* `binance` / `disconnected`: `985` events from `2026-04-13 16:34:59 UTC` to `2026-04-24 18:26:35 UTC`
* `clob` / `disconnected`: `942` events from `2026-04-13 01:15:31 UTC` to `2026-04-24 18:29:44 UTC`
* `chainlink` / `connected`: `230` events from `2026-04-12 15:34:53 UTC` to `2026-04-24 18:19:14 UTC`
* `chainlink` / `disconnected`: `229` events from `2026-04-12 17:34:53 UTC` to `2026-04-24 18:19:13 UTC`
* `chainlink` / `stale`: `61` events from `2026-04-13 08:35:10 UTC` to `2026-04-24 02:56:23 UTC`

## Log Counters

* `paint.log`: `349,208` lines, `6,996` warn-like hits, `2,735` error-like hits, `2,156` feed disconnects, `26,922` rejection rollups, `3,204` readonly rollups, `0` account refresh failures
* `bot_wrapper.log`: `349,207` lines, `6,996` warn-like hits, `2,735` error-like hits, `2,156` feed disconnects, `26,922` rejection rollups, `3,204` readonly rollups, `0` account refresh failures
* `sidecar.log`: `16,544` lines, `3,150` warn-like hits, `3` error-like hits, `0` feed disconnects, `0` rejection rollups, `0` readonly rollups, `0` account refresh failures
* `agent.log`: `1` lines, `0` warn-like hits, `0` error-like hits, `0` feed disconnects, `0` rejection rollups, `0` readonly rollups, `0` account refresh failures
* `dashboard.log`: `2` lines, `0` warn-like hits, `0` error-like hits, `0` feed disconnects, `0` rejection rollups, `0` readonly rollups, `0` account refresh failures

## Read

The no-trade period after the final trade is not explained by missing collectors. Feed and signal rows continue after the halt. The health work item remains to reduce CLOB churn and sidecar reconnect noise because those events add operational noise and can contribute to stale-feature rejections.
