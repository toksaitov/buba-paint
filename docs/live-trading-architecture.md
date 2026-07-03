# Live Trading Architecture

This chapter documents the live-capable architecture in the local tree. It is advanced context for future funded work. It is not permission to deploy or arm real money.

## Modes

The mode boundary is `Config::execution_mode`.

* `paper`: simulated execution, backtests, sweeps, and ordinary research.
* `live_readonly`: authenticated venue/account reads plus the shared shadow paper runtime.
* `live_trading`: local disarmed real-venue runtime with ledger, reconciliation, risk halt, sidecar write boundary, and audited controls.

The design steady state for remote operation is `live_readonly`. The live host is currently staged off it in the disarmed, parked `live_trading` canary for the live-readiness effort; see the handoff block in [../LIVE_READINESS_PLAN.md](../LIVE_READINESS_PLAN.md).

## Shared Decision Core

Paper, readonly, backtest, and future live paths share one strategy core.

Shared components:

* market discovery
* public feed ingestion
* feature engine
* strategy evaluation
* bankroll and reserve policy
* exposure checks
* rejection tracking
* signal telemetry
* replay and backtest tooling

The hot path updates in-memory state, evaluates the latest decision snapshot, and enqueues bounded work. It must not run SQLite scans, replay validators, dashboard aggregation, account polling, control polling, sidecar calls, or venue submission.

## Venue Boundary

The venue boundary is intentionally narrow. The current code exposes three behavioral roles rather than three public venue types:

* paper execution handles simulated orders and settlement
* live-readonly execution handles authenticated account, preflight, metadata, activity, and reconciliation reads
* live write execution handles real order, cancel, redemption, fill, and reconciliation surfaces behind explicit arming

In `live_readonly`, the bot creates readonly live sessions, polls account/activity/preflight state through the sidecar, persists account snapshots and reconciliation facts, and continues shadow paper trading. It does not submit, cancel, redeem, or arm venue actions.

In `live_trading`, the process starts disarmed. Live submission is routed through a worker that persists critical decision evidence and live order intent before any sidecar call. Unknown submission outcomes, stale account truth, user-stream failure, replay capture failure, terminal halt state, or critical reconciliation block new risk.

## Sidecar Contract

`polymarket-sidecar` isolates private venue credentials and authenticated Polymarket calls from the Rust bot. It is a private service in Docker Compose and is not exposed to the browser.

Local HTTP routes:

* `GET /health`
* `GET /account`
* `GET /activity`
* `POST /preflight`
* `POST /orders`
* `POST /cancel`
* `POST /cancel-all`
* `POST /redeem-all`

Implemented responsibilities:

* CLOB V2 auth bootstrap
* geoblock, clock, account, collateral, allowance, and market metadata checks
* pUSD collateral diagnostics
* authenticated user-stream and activity recovery
* FOK/FAK order submission boundary
* single-order cancel and cancel-all boundary
* pUSD CTF redemption boundary
* raw-safe failure classification and details
* secret redaction

The active sidecar packages are `@polymarket/clob-client-v2`, `@polymarket/builder-relayer-client`, and `@polymarket/builder-signing-sdk`. The configured proxy-wallet model uses `POLYMARKET_SIGNATURE_TYPE=1`; `POLYMARKET_FUNDER` defaults to `POLYMARKET_PROXY_WALLET` when omitted.

Dashboard controls never call these sidecar routes directly. Controls go through dashboard server, agent, the bot control ledger, and the running bot.

## Ledger And Control

Live state is persisted as ledger facts, not inferred from one balance number.

The live schema records:

* sessions
* account snapshots
* order intents
* venue orders
* fills
* redemptions
* reconciliation events
* control state
* control commands and audit

`live-control` queues audited commands into the DB. Supported actions are `preflight`, `arm`, `disarm`, `stop-after-flat`, `kill-switch`, `cancel-all`, and `redeem-all`. The running bot applies commands; the agent and dashboard only enqueue and observe.

`live-closeout` exports evidence after terminal halt or funded-session shutdown. It does not clear halted state or make a DB re-armable.

## Risk And Halt

Future live risk is enforced from authoritative account snapshots plus the local ledger.

Tracked risk state includes:

* session start equity
* UTC-day baseline equity
* high-water mark
* trough
* current equity
* daily loss
* session drawdown
* percentage drawdown
* terminal halt reason

Current defaults are `LIVE_MAX_DAILY_LOSS_USD=15`, `LIVE_MAX_SESSION_DRAWDOWN_USD=20`, and the existing `MAX_DRAWDOWN_PCT`.

Terminal halt is persistent. Drawdown breach, daily-loss breach, auth/geoblock failure, storage failure, replay capture failure, critical reconciliation, unresolved unknown order state, or prolonged account/user-stream/venue degradation blocks new submissions. Restarting `live_trading` against terminal live state fails fast.

## Replay And Fidelity

Replay-grade public capture remains mandatory. It is not enough for funded research.

Funded intervals need both:

* `validate-replay-data` reporting public `sweep_grade`
* `validate-live-fidelity` reporting private `research_grade_live`

`research_grade_live` requires explainable decision evidence, market metadata, token ID, order fields, client order ID, submit/ack/update timing, venue state, fills/cancels/unknowns, account snapshots, reconciliation events, and control audit.

Even then, replay is not an exact exchange simulator. Queue position, hidden liquidity, matching-engine internals, network path differences, and relayer timing are not fully reconstructable.

## Safe Workflow

Current safe workflow:

1. use `paper` for ordinary research and dashboard work
2. use Docker/Caddy `live_readonly` for authenticated readonly operation
3. run `live-preflight` only as a readonly readiness check
4. use `live_trading` only in local or mocked verification unless a fresh funded plan explicitly approves otherwise
5. use `live-control` and dashboard Execution controls only as bot-ledger controls, never as direct sidecar controls
6. use `live-closeout` after terminal halt or funded-session shutdown
7. validate replay, backtest input, and live fidelity offline before using funded data for research

The architecture exposes real venue-action boundaries. The design steady state remains no-order `live_readonly`, but the live host is currently staged in the disarmed, parked `live_trading` canary for the live-readiness effort; see [../LIVE_READINESS_PLAN.md](../LIVE_READINESS_PLAN.md).
