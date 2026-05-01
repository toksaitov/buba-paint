# Live Session Runbook

This runbook describes the intended operator workflow for the first real-money proxy-wallet pilot. It also describes the current local limits so nobody confuses readonly venue monitoring with production live trading.

## Current repository state

What is ready locally:

- explicit execution modes: `paper`, `live_readonly`, `live_trading`
- live preflight CLI
- sidecar package and typed authenticated-venue boundary
- real `live_readonly` runtime inside `buba-paint live`
- live session and reconciliation tables
- agent and dashboard live-readiness surfaces
- replay-grade public feed capture for research runs
- compact live account telemetry schema
- shadow paper analysis pages during readonly sessions
- sidecar readiness and crash diagnostics on `/health`
- sidecar FOK/FAK order, cancel, cancel-all, and pUSD CTF redemption boundary
- sidecar sanitized activity recovery on `/activity`
- local disarmed `live_trading` runtime and audited `live-control` command queue for mocked verification

What is still intentionally gated:

- `EXECUTION_MODE=live_trading` is not deployed or operator-approved for real money
- dashboard arming, disarming, kill-switch, and redemption controls are not enabled yet
- host rollout, funded canary checks, and dashboard control UX are unfinished
- readonly monitoring uses live account reads, user-stream health, and sanitized activity recovery, but the funded operating procedure is not complete yet

Do not deploy real-money trading from this state. Use it to finish implementation and validate readiness safely.

## Preflight checklist

Before any future live-money session:

1. confirm host geoblock status from the actual deployment host
2. confirm `POLY_PROXY` credentials are present:
   - exported Polymarket private key
   - proxy wallet address
   - builder relayer credentials required for redemption
   - optional funder override only if it must differ from the proxy wallet
3. confirm local clock drift is below the configured threshold
4. confirm venue min order size, tick size, and fee metadata for the active market set
5. confirm configured cash caps permit at least one legal order
6. confirm live mode is latency-arb only for the first pilot
7. confirm dashboard Execution page and agent live endpoints show healthy readiness state
8. confirm the sidecar is supervised and auto-restart capable on the host

## Recommended first pilot envelope

The first real-money canary should stay narrow:

- bankroll target: `75-100 USD`
- `latency-arb` only
- very small per-order cap
- tight open-notional cap
- aggressive session loss and drawdown limits
- short session, typically `2-3` days, with manual stop allowed earlier once enough data is collected

The budgeting rule should be conservative:

- `tradable_cash = min(actual_cash_available, LIVE_SESSION_CASH_CAP_USD)`

## Session lifecycle

Intended operator lifecycle once live trading is actually enabled:

1. start in `live_readonly`
2. pass preflight
3. inspect budget preview, strategy set, geoblock, auth, account state, and user-stream health
4. arm live trading explicitly from the approved control surface
5. monitor open orders, fills, reconciliation warnings, and redeemable inventory
6. if drawdown or divergence persists too long, disarm and stop after flat
7. redeem winning resolved positions
8. export official accounting/activity data after the session
9. feed the collected live data back into paper and backtest parity improvements

## Data to retain after each session

Keep these artifacts:

- local SQLite DB with replay-grade public feed capture and compact live telemetry
- bot log
- agent/dashboard logs if relevant
- official Polymarket accounting and activity exports
- any rotated forensic private-payload files if that mode was enabled intentionally

Do not bloat SQLite with full raw private websocket traffic unless a short forensic session explicitly requires it.

## UI safety rules

The dedicated Execution page should enforce:

- process control separated from trading control
- typed confirmation before arming
- full config fingerprint displayed before arming
- mobile restricted to observe and disarm only
- immediate disarm availability
- clear `cancel all`, `redeem all`, and `stop after flat` actions

Any of these should block or disarm trading automatically:

- geoblock failure
- auth failure
- user-stream outage
- reconciliation red state
- configured risk cap trip

## Release checklist before enabling real money

Before the live venue runtime is considered ready:

- Rust workspace builds cleanly
- Rust tests pass
- dashboard client tests and build pass
- sidecar lint, tests, and build pass
- sidecar supervision artifact and stable env or log layout are in place on the deployment plan
- `ops/` service templates have been reviewed for sidecar, bot, agent, and dashboard
- replay-data quality is checked before any parameter sweep with `validate-replay-data`
- docs are updated and internally consistent
- comments and rustdoc are current
- agent and dashboard live surfaces are verified against real readonly session data
- live-readonly soak completes without storage blow-up

Only after that should a real deployment plan be written.
