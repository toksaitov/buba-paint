# 90-Minute Soak: Data Analysis

Health analysis of run `colleague-20260503-060213Z` (90-minute no-order `live_readonly` soak on `buba-paint-fin`, soak window `2026-05-03T06:06:15Z` to `2026-05-03T07:37:54Z`). All evidence under [data/experiments/replay-grade-readonly-soak-colleague-002/](data/experiments/replay-grade-readonly-soak-colleague-002/). Companion docs: [REPORT.md](REPORT.md) for what changed, [LOGS.md](LOGS.md) for where to read raw logs, [data/experiments/replay-grade-readonly-soak-colleague-001/probes/probe-report.md](data/experiments/replay-grade-readonly-soak-colleague-001/probes/probe-report.md) for the auth probe.

## Verdict

Healthy across every dimension PROMPT.md gates on. No observable degradation between minute 5 and minute 90.

## Service health

All 21 captured health snapshots (`pre-soak-health.json` plus `poll-01..18-health.json` plus `post-soak-health.json`) returned the same shape:

- `ready=true`
- `user_stream_status=ok`
- `consecutive_user_stream_failures=0`
- `last_user_stream_disconnect_reason=null`
- `last_account_refresh_error=null`
- `auth_configured=true`
- `relayer_api_key_present=true`

`post-stop-health.json` returned empty bodies, which is the expected shape when the supervised services are stopped.

## Auth

`auth_bootstrap_start` and `auth_bootstrap_ok` fired exactly once each at sidecar startup (about 2 seconds apart). After that the sidecar held the same L2 credentials for the entire 91-minute wall-clock run. Zero re-bootstraps, zero auth errors, zero credential refreshes.

## User stream

Deduplicated sidecar event counts across the full run:

| Event | Count |
|---|---|
| `user_stream_connect_start` | 19 |
| `user_stream_connect_ok` | 19 |
| `user_stream_connect_cancelled` | 18 |
| `user_stream_disconnect` | 0 |
| `user_stream_error` | 0 |
| `user_stream_unhealthy` | 0 |

19 connects = 1 initial connection plus 18 reconnects. That matches exactly the 18 market-window rotations in 90 minutes (one BTC up/down market closes and a new one opens every 5 minutes). Every cancellation reason was `subscription_updated`, the venue's required re-subscribe pattern when the active market set changes; never a failure or a stall. The previously observed "silent freeze" pattern on the V2 user channel did not occur once.

## Account refresh

The sidecar refreshes Polymarket account state on a ~30-second timer. Across the 18 polls, `last_account_refresh_age` cycled cleanly between 0 and 55 seconds (just-after-refresh to just-before-next-refresh), with no jumps beyond the period and no errors. `cash_available` and `allowance_available` both held at `$99.166994` for the entire run, matching the L1 preflight value at the start.

## Feed throughput

`validate-replay-data` returned `replay_quality=sweep_grade` for the **entire 5499-second soak window**, not a sample. Total 323,187 feed events = 58.8 events/sec sustained.

| Feed class | Rows | Rate |
|---|---|---|
| binance_book_ticker | 201,953 | 36.73/s |
| binance_depth | 21,795 | 3.96/s |
| clob_up_top_of_book | 20,581 | 3.74/s |
| clob_down_top_of_book | 20,569 | 3.74/s |
| binance_agg_trade | 19,880 | 3.62/s |
| chainlink_price | 5,431 | 0.99/s |

The Binance book-ticker at ~37/s and the Chainlink price feed at ~1/s are both at expected rates for BTC during a calm 90-minute window in early-Sunday US hours. CLOB up/down counts are essentially symmetric (20581 vs 20569, 0.06% difference): both legs got equal attention from the venue's price-change events. No feed class fell silent for any extended period; the bot's own internal `validate-replay-data` tagged `replay_quality_class=sweep_grade` at every poll capture.

## Strategy behavior

Latency-arb (the only strategy enabled in the soak's bot env) evaluated 22,960 to 122,468 times per market window, with the count growing as the window stayed open longer. The five sampled `strategy rejection rollup` lines:

| Market | Evaluations | Top reasons | quoteAgeMs | realizedVol15sBps | moveVelocity |
|---|---|---|---|---|---|
| 2138280 | 24,497 | direction_not_selected=99.0%, features_stale=0.9% | 16 | 0.01 | 0.000005 |
| 2138280 | 96,994 | direction_not_selected=79.5%, window_too_late=20.1% | 9 | 0.01 | 0.000042 |
| 2138297 | 112,934 | direction_not_selected=73.6%, window_too_late=26.1% | 8 | 0.05 | 0.000103 |
| 2138305 | 122,468 | direction_not_selected=74.6%, window_too_late=25.1% | 7 | 0.03 | 0.000082 |
| 2138342 | 22,960 | direction_not_selected=98.6%, features_stale=1.4% | 21 | 0.00 | 0.000004 |

The whole run sat in this regime: top rejection always `direction_not_selected` (the strategy looked at features and chose to skip), with `window_too_late` taking 20-26% as windows aged toward expiry. `features_stale` stayed under 2% throughout, which is normal noise. `book_unavailable` was effectively zero. Quote age stayed at 7-21 ms (fresh feeds), and `realizedVol15sBps` of 0.00-0.05 means the underlying market was essentially flat for the entire 90-minute window.

`paper execution rollup` for every window:

```
submitted=0 filled=0 missed=0 rejected_before_queue=0 partial=0
```

Zero paper trades fired. That is the correct strategy response to this regime; the bot watched the right signals and correctly chose not to act.

## Live ledger

The PROMPT.md safety gates:

- `live_order_intents`: 0
- `live_orders`: 0
- `live_fills`: 0
- `live_redemptions`: 0
- `live_sessions` with `execution_mode='live_trading'`: 0

The only `live_sessions` row was `live_readonly:readonly_ready:1`. The runtime never entered `live_trading`, never queued an order, never reached for builder-relayer credentials. Acceptance check (`17-remote-acceptance-check.log`) printed `acceptance_check=ok`.

## Errors and warnings

- 0 sidecar `level=error` events
- 0 sidecar `level=warn` events
- 0 bot `ERROR` log lines
- 0 bot `WARN` log lines

across all 18 poll log tails plus the pre-soak and post-soak captures.

## Closeout and preservation

- `post-stop-processes.txt` lists zero loaded `buba-*` user units. Services down cleanly, no zombie processes from earlier releases.
- `post-stop-db.txt` reproduces `quick_check=ok` and the same zero counts on live tables.
- Remote runtime preserved at `/root/buba-paint-live/runtime/soak-001-colleague-20260503-060213Z` (71 MB).
- All prior runtimes preserved on the host: the May 2 `soak-004` runtime, the failed first 5-minute attempt, the passed 5-minute run, and now this 90-minute run. PROMPT.md's "Do not delete prior remote runtimes" rule is satisfied.

## Dashboard summary

`dashboard-trading-summary.json` was captured for pre-soak, every poll, and post-soak (the empty-summary regression from the 5-minute run is fully fixed by patch A2 in [REPORT.md](REPORT.md)). Key fields from the post-soak capture:

- `runtime_mode`: `live_readonly`
- `trading_state`: `readonly`
- `account_health.label`: `Account tracked`
- `venue_health.label`: `Venue connected`
- `reconciliation_health.label`: `Reconciliation clean`

The dashboard's view of the bot matched the bot's own self-reported state at every capture.

## One nuance worth flagging

Zero paper trades fired across 90 minutes. That is the correct strategy response to a calm regime, but it means this soak validated the readonly observation path end-to-end without exercising the active paper-trade execution path under live signals. PROMPT.md scope explicitly asks for that ("no orders, just auth and account info"), so it is not a gap against the spec. For a future funded-canary phase you would want a soak that overlaps with a more volatile window or a longer duration to get organic strategy fires and validate the execution path under signal load.

The other consideration is that builder relayer credentials are still absent from `.secrets/buba-paint-live-sidecar.env`, so redemption stays fail-closed (`redemption_readiness=unavailable_missing_builder_credentials`). That is by design for a readonly soak, called out here for visibility before any phase that needs to redeem.
