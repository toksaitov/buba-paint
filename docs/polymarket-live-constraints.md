# Polymarket Live Constraints

This document captures the venue facts that matter for the first real-money pilot. Update it whenever Polymarket changes account rules, fees, market metadata, or live trading restrictions.

## Account model

For email, Google, or Magic Link Polymarket accounts:

- account type is `POLY_PROXY`
- signature type is `1`
- API trading requires exporting the private key from Polymarket
- the proxy wallet shown by Polymarket is the effective funder wallet
- if `POLYMARKET_FUNDER` is omitted locally, the sidecar should default it to the proxy wallet

For this account type, the relayer and gasless path is the supported operational model for approvals and redemption.

## SDK support

Official client coverage is uneven:

- CLOB client support exists in Rust, TypeScript, and Python
- relayer SDK support exists in TypeScript and Python

Because of that split, the repo uses a TypeScript sidecar for the full authenticated venue boundary instead of trying to mix Rust CLOB calls with ad hoc redemption code. The active sidecar CLOB dependency is `@polymarket/clob-client-v2`. Legacy V1 CLOB, order-utils, and builder-signing packages are not part of the readonly CLOB boundary.

## CLOB V2 and collateral

Polymarket CLOB V2 is the production venue contract. V1-signed orders are not accepted. The sidecar must use the V2 client shape, `createOrDeriveApiKey`, V2 signature types, and the current proxy-wallet funder model.

Trading collateral is pUSD. The code still stores account values as USD-denominated numbers with 6-decimal collateral units, but docs and UI should not imply USDC.e is the current trading collateral.

Implications:

- no signed `feeRateBps` field in new live order logic
- no bot-managed order nonce in new live order logic
- no V1 taker-field signing assumptions
- account diagnostics should identify the collateral model as pUSD
- balance and allowance checks must use the current CLOB collateral contract path exposed by the V2 client

## Geoblock and hosting

Geoblock checks must be performed from the actual host that will run the bot.

Current official guidance:

- `GB` is blocked
- `IE` is not listed as blocked
- `eu-west-1` is the recommended nearby non-georestricted region

The host must pass the official geoblock endpoint at startup and again before any future live arming step.

The `2026-05-01` no-order host check returned `blocked=false` from the geoblock endpoint, but Gamma BTC 5-minute event discovery returned HTTP `403` with body `error code: 1010` from the same host. Treat host-safe market discovery as unresolved until a later check proves Gamma or an approved fallback works from the deployment environment.

## Matching-engine restart window

Polymarket documents a weekly matching-engine restart on Tuesday at `7:00 AM ET`. During that window, trading endpoints can return HTTP `425`.

Implication:

- live order submission must treat `425` as a temporary venue restart, not as a fatal bug
- the bot should pause submission, continue monitoring, and retry only after venue health returns

## User WebSocket

Authenticated user WebSocket usage is server-side only. It must not be handled by the browser dashboard.

The sidecar is the correct place for:

- order lifecycle events
- trade/fill events
- future reconciliation triggers

## Current 5-minute BTC market behavior

Recent live inspection of BTC 5-minute markets showed:

- `orderMinSize = 5`
- `orderPriceMinTickSize = 0.01`
- `feesEnabled = true`
- `feeSchedule = { exponent: 1, rate: 0.072, takerOnly: true, rebateRate: 0.2 }`
- `rewardsMinSize = 50`
- `rewardsMaxSpread = 4.5`
- `clearBookOnStart = false`
- markets can be visible and `acceptingOrders=true` well before the actual 5-minute window begins

Implications:

- do not assume the order book clears on start
- do not assume discovery only matters immediately before the slot
- live budgeting must respect venue min size and tick size before arming
- before arming real money, revalidate these values from the deployment host because local Gamma requests may be geoblocked

## Fee ambiguity

Current venue observations are internally inconsistent:

- market objects report `feeSchedule.rate = 0.072`
- `GET /fee-rate?token_id=...` returned `{"base_fee":1000}` for the same BTC 5-minute tokens

Implication:

- fees must be modeled as runtime venue truth plus reconciliation
- the bot should store market fee metadata from the V2 CLOB surface and any explicit fee-rate endpoint responses at intent time
- paper and backtest parity should use the same fee-resolution path, not hardcoded historical constants only

## Redemption and cash availability

Winning positions do not instantly become spendable cash:

- markets first resolve
- positions become redeemable
- redemption must be submitted
- only after redemption settles and the remote balance reflects it can cash be treated as available

Official help still documents delayed or failed claims caused by:

- clock mismatch
- unsupported-region access
- congestion and relay delays

Implications:

- live mode needs a clock-drift preflight
- cash budgeting in v1 should use only observed `cash_available`
- expected redemption proceeds must stay non-spendable until the account state confirms credit
- Bridge withdrawal automation is out of the first live-money implementation unless it receives a separate implementation and test plan

## First pilot operating rules

The first real-money pilot should assume:

- bankroll target `75-100 USD`
- legal cash may be slightly below `100`
- `latency-arb` only
- taker-only flow
- strict per-order and open-notional caps
- automatic disarm on auth, geoblock, reconciliation, or venue-health failure

The code should stay architecture-ready for calm and spread later, but the operating policy should remain narrow until real-money data proves otherwise.
