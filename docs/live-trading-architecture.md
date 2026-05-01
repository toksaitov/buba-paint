# Live Trading Architecture

This document describes the real-money architecture that now exists in the local tree.

The system has three execution modes:

- `paper`: the current production-quality paper trading and backtest environment
- `live_readonly`: authenticated venue/account monitoring plus the shared shadow paper runtime, still without order placement
- `live_trading`: reserved for real venue order flow once the dedicated live venue runtime is finished

The mode boundary is explicit in `Config::execution_mode`. The goal is to keep the strategy core shared while isolating venue-specific risk.

## Shared bot core

These parts stay shared across paper, live-readonly, and future live-trading:

- market discovery
- public feeds
- feature engine
- strategy evaluation
- portfolio router
- reserve and bankroll policy
- rejection tracking
- signal telemetry
- backtest parity tooling

This keeps parameter research, paper runs, and live runs on one decision engine instead of three diverging code paths.

## Venue boundary

The venue boundary is intentionally narrow:

- `PaperVenue`: current simulated order submission and settlement path
- `LiveReadonlyVenue`: authenticated account and venue state, but no order placement
- `LiveVenue`: future real order submission, fills, redemption, and reconciliation

In the current local tree, `buba-paint live` supports `EXECUTION_MODE=live_readonly` as a real authenticated venue/account monitor layered on top of the shared paper runtime. It creates readonly live sessions, polls live account state, persists account snapshots, logs reconciliation events, and continues to generate shadow paper signals/trades/equity without placing real orders. `EXECUTION_MODE=live_trading` is still intentionally gated so real order flow cannot start by accident.

## Authenticated sidecar

The authenticated Polymarket boundary lives in `polymarket-sidecar/`.

Reason:

- official Rust support covers the CLOB API
- official relayer and gasless redemption support is TypeScript and Python only
- proxy-wallet accounts are safest when the credential-heavy boundary is isolated from the main bot runtime

The sidecar owns:

- CLOB V2 auth material
- relayer auth
- private user-stream connectivity
- allowance and account checks
- future real order placement
- future redemption submission

In the current local tree, the sidecar has a real readonly provider for:

- `GET /health`
- `GET /account`
- `POST /preflight`

These endpoints use real proxy-wallet auth, host geoblock checks, account-state reads, active-market discovery, CLOB V2 market metadata, pUSD collateral diagnostics, and authenticated user-stream connectivity. The write endpoints still return explicit not-implemented responses in this pass.

The sidecar now also carries its own crash-resistance and readiness model:

- websocket user-stream lifecycle is explicit and reconnect-safe
- auth bootstrap failures are not cached forever
- health stays live at `200` but includes additive readiness fields
- account refresh remains fail-closed for money and order facts
- graceful shutdown and fatal-process logging are part of the process model

The Rust bot talks to the sidecar over a private local HTTP contract:

- `GET /health`
- `GET /account`
- `POST /preflight`
- `POST /orders`
- `POST /cancel`
- `POST /cancel-all`
- `POST /redeem-all`

The sidecar now implements real readonly-safe venue checks on the health, account, and preflight routes. It does not place real orders yet, and the write-path endpoints still return explicit not-implemented responses.

The active CLOB client package is `@polymarket/clob-client-v2`. The sidecar uses the V2 constructor shape, V2 signature types, and `createOrDeriveApiKey` for L1-to-L2 auth bootstrap. It no longer depends on V1 CLOB, order-utils, or builder-signing packages for readonly venue access.

The preferred host process model is no longer an unsupervised `nohup node` process. The target deployment shape is:

- sidecar code from `~/buba-paint-live/current/polymarket-sidecar`
- env from `~/buba-paint-live/config/sidecar.env`
- logs at `~/buba-paint-live/logs/sidecar.log`
- supervised restart policy

## Proxy-wallet account model

The first real-money pilot is designed for a Polymarket email or Magic Link account:

- account type: `POLY_PROXY`
- signature type: `1`
- credentials: exported Polymarket private key
- wallet/funder: the proxy wallet shown by Polymarket (the local sidecar config defaults `POLYMARKET_FUNDER` to `POLYMARKET_PROXY_WALLET` when it is omitted)
- gasless path: relayer API key

The sidecar config reflects that account model directly.

The current collateral model is pUSD. Internal account values remain USD-denominated numbers derived from 6-decimal collateral units. User-facing copy should say pUSD or collateral where the venue-specific distinction matters.

## Strategy readiness

The implementation target is live capability for all strategy families. The initial funded rollout policy is intentionally narrower: enable `latency-arb` only by runtime config and keep `calm-persistence` and `spread-capture` disabled until real-money data and residual-exposure handling justify enabling them.

The readiness matrix is surfaced in the sidecar preflight request and should stay aligned with actual rollout policy. Spread capture is not atomic because each leg is an independent order, so it needs explicit residual-exposure handling before it can be enabled with real money.

## Live ledger and telemetry

Live-money state is not modeled as one balance number. The additive live tables capture:

- live sessions
- order intents
- venue orders
- fills
- account snapshots
- redemptions
- reconciliation events
- control audit actions

The intended cash model is:

- `cash_available`
- `cash_reserved_for_orders`
- `inventory_mark_value`
- `redeemable_value`
- `pending_redeem_value`
- `total_equity`

The first real-money pilot may spend only `cash_available`.

Private live-account SQLite capture must stay compact. Public market feed capture for research runs should use `FEED_EVENT_STORAGE_PROFILE=replay_grade` so future sweeps have the Binance book state needed for parity:

- one row per state transition
- periodic account snapshots every 60s
- event-driven snapshots after order, fill, redeem, and reconcile transitions
- optional forensic raw private payload capture only as rotated compressed files outside SQLite

## Fee handling

Fee handling can no longer be hardcoded safely.

The sidecar readonly preflight now includes CLOB V2 market fee metadata where available. The live order path must persist fee metadata at intent time before real order placement is enabled. Paper mode and backtests should use the same fee-resolution path as the live-readiness surfaces. This is necessary because live venue data has shown fee-surface inconsistency between market objects and the `fee-rate` endpoint for BTC 5-minute markets.

## Current safe workflow

Local-only safe workflow:

1. keep `EXECUTION_MODE=paper` for actual trading decisions
2. run `live-preflight` with `EXECUTION_MODE=live_readonly` against the sidecar
3. run `buba-paint live` with `EXECUTION_MODE=live_readonly` for an authenticated readonly soak
4. inspect live readiness in the dashboard Execution page and agent live endpoints
5. do not arm or deploy real trading until the dedicated live venue runtime and redemption path are fully wired

This repository state is intentionally staged for correctness. It exposes a real readonly venue boundary without pretending that live order flow is ready before the live trading runtime exists.
