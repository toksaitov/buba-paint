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

- CLOB auth material
- relayer auth
- private user-stream connectivity
- allowance and account checks
- future real order placement
- future redemption submission

In the current local tree, the sidecar has a real readonly provider for:

- `GET /health`
- `GET /account`
- `POST /preflight`

These endpoints use real proxy-wallet auth, host geoblock checks, account-state reads, active-market discovery, and authenticated user-stream connectivity. The write endpoints still return explicit not-implemented responses in this pass.

The Rust bot talks to the sidecar over a private local HTTP contract:

- `GET /health`
- `GET /account`
- `POST /preflight`
- `POST /orders`
- `POST /cancel`
- `POST /cancel-all`
- `POST /redeem-all`

The sidecar now implements real readonly-safe venue checks on the health, account, and preflight routes. It does not place real orders yet, and the write-path endpoints still return explicit not-implemented responses.

## Proxy-wallet account model

The first real-money pilot is designed for a Polymarket email or Magic Link account:

- account type: `POLY_PROXY`
- signature type: `1`
- credentials: exported Polymarket private key
- wallet/funder: the proxy wallet shown by Polymarket (the local sidecar config defaults `POLYMARKET_FUNDER` to `POLYMARKET_PROXY_WALLET` when it is omitted)
- gasless path: relayer API key

The sidecar config reflects that account model directly.

## Strategy readiness

The architecture is ready for all strategy families, but the initial live rollout policy is intentionally narrow:

- `latency-arb`: `live_ready_v1`
- `calm-persistence`: `live_supported_but_disabled`
- `spread-capture`: `not_live_v1`

This readiness matrix is surfaced in the sidecar preflight request and should stay aligned with the actual rollout policy.

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

SQLite capture must stay compact:

- one row per state transition
- periodic account snapshots every 60s
- event-driven snapshots after order, fill, redeem, and reconcile transitions
- optional forensic raw private payload capture only as rotated compressed files outside SQLite

## Fee handling

Fee handling can no longer be hardcoded safely.

The bot now persists market-level fee metadata and per-token fee-rate responses when discovery loads a market. Paper mode and backtests use the same fee-resolution path as the live-readiness surfaces. This is necessary because live venue data currently shows a fee-surface inconsistency between market objects and the `fee-rate` endpoint for BTC 5-minute markets.

## Current safe workflow

Local-only safe workflow:

1. keep `EXECUTION_MODE=paper` for actual trading decisions
2. run `live-preflight` with `EXECUTION_MODE=live_readonly` against the sidecar
3. run `buba-paint live` with `EXECUTION_MODE=live_readonly` for an authenticated readonly soak
4. inspect live readiness in the dashboard Live page and agent live endpoints
5. do not arm or deploy real trading until the dedicated live venue runtime and redemption path are fully wired

This repository state is intentionally staged for correctness. It exposes a real readonly venue boundary without pretending that live order flow is ready before the live trading runtime exists.
