# Live Session Runbook

This runbook describes the intended workflow for a future funded Polymarket pilot. It is not an instruction to arm real money. Current remote operation remains Docker/Caddy `live_readonly`.

## Current Status

The repository supports three modes: `paper`, `live_readonly`, and disarmed `live_trading`. The deployed path should use `live_readonly` unless a fresh funded-run plan explicitly changes it.

Current safe uses:

* paper research and dashboard work
* authenticated readonly venue/account monitoring
* replay-grade public feed capture
* shadow paper strategy evaluation during readonly sessions
* local or mocked verification of live-control, ledger, halt, closeout, and sidecar write boundaries

Current non-goals:

* no deployed `live_trading`
* no real-money arming
* no venue order placement, cancellation, or redemption from the remote runtime
* no current funded canary

## Future Funded Prerequisites

Before any funded session is planned, verify the deployment host and account from the actual runtime environment.

Required checks:

* host geoblock result from the real deployment host
* sidecar `/health`, `/account`, `/activity`, and `/preflight`
* proxy-wallet or deposit-wallet account model, signature type, funder, and pUSD collateral state
* CLOB market metadata: token IDs, tick size, min size, accepting-order state, fee metadata, and neg-risk fields
* user-stream health and authenticated activity recovery
* replay capture health for Binance, Chainlink, and CLOB UP/DOWN top-of-book evidence
* dashboard Execution state, Parameters snapshot, and Machine page health
* DB quick check and offline replay/backtest gates after a readonly soak

If any host, venue, account, data, or reconciliation fact is unknown, the funded plan stops until that fact is resolved.

## Future Pilot Envelope

Any future first funded pilot should stay narrow.

* bankroll around `$100`
* latency-arb only
* calm-persistence disabled
* spread-capture disabled
* FOK/FAK only
* small single-order cap
* tight open-notional, daily-loss, and session-drawdown caps
* terminal halt on unknown order state, critical reconciliation, or persistent venue/account/user-stream degradation

The spending rule is:

```text
tradable_cash = min(actual_cash_available, LIVE_SESSION_CASH_CAP_USD)
```

Pending settlement is not spendable cash.

## Session Lifecycle

Intended sequence for a future funded session:

1. deploy a reviewed release in `live_readonly`
2. run readonly preflight and confirm host/account/market/user-stream state
3. confirm the Parameters page matches the approved runtime profile
4. confirm replay capture is healthy and recent
5. start a fresh `live_trading` run DB only after an approved funded plan
6. arm explicitly through the audited control surface
7. monitor account state, venue state, user-stream state, fills, reconciliation, and risk caps
8. disarm or stop-after-flat when evidence quality or risk state degrades
9. allow only safe cleanup controls after a terminal halt
10. run `live-closeout` and complete the postmortem before any later funded run

A halted or `unknown_order` DB must not be re-armed. The next funded attempt uses a new run DB.

## Terminal Halt And Closeout

Terminal halt is a session boundary. It is not a paused state.

Terminal triggers include:

* `LIVE_MAX_SESSION_DRAWDOWN_USD`
* `LIVE_MAX_DAILY_LOSS_USD`
* `MAX_DRAWDOWN_PCT`
* auth or geoblock failure while armed
* replay capture failure while armed
* storage failure while armed
* unresolved unknown order state
* critical reconciliation
* account, user-stream, or venue degradation lasting beyond the configured terminal threshold

After a terminal halt:

* do not arm the same DB
* queue only cleanup controls that the ledger/account state says are safe
* export closeout evidence with `live-closeout`
* write the postmortem before any later funded plan

The closeout package should include the SQLite quick-check result, replay-quality report, live-fidelity report, live ledger exports, account snapshots, reconciliation events, control audit, logs, and a postmortem stub.

## Data Retention

Keep:

* run SQLite DB
* bot log
* sidecar, agent, and dashboard logs when relevant
* closeout manifest and summary
* official Polymarket accounting or activity exports used for reconciliation
* readonly or funded evidence bundles under `data/experiments/...`

Do not use a configured storage profile as proof of data quality. `FEED_EVENT_STORAGE_PROFILE=replay_grade` is only a capability. The captured interval must still pass `validate-replay-data`, and sweep inputs must also pass `validate-backtest-input`. Funded intervals also need `validate-live-fidelity`.

## UI Safety

The dashboard Execution page is an operator surface, not a venue client.

Required UI behavior:

* process state is separate from trading state
* running never means armed
* controls queue commands through the bot ledger
* dashboard never calls the sidecar or venue directly
* arming requires explicit confirmation and current readiness gates
* halted state dominates the page
* cleanup actions are visible only when capability data says they are safe

Mobile may be used for observation and emergency-safe controls only when the approved funded plan allows it. It should not become the primary arming surface.

## Release Checklist

Before writing any funded plan, verify:

* Rust workspace tests and release build
* sidecar lint, tests, and build
* dashboard tests and build
* hot-path audit
* docs and comment audits
* local Docker smoke
* host no-order readonly soak
* `validate-replay-data`
* `validate-backtest-input`
* dashboard Execution, Parameters, Logs, and Machine pages
* explicit operator approval for the funded envelope

Only after those checks should a new real-money plan be written.
