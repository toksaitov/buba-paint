# Polymarket Live Constraints

This chapter records venue facts that affect the sidecar and any future funded trading plan. These facts are unstable by nature. Re-check official Polymarket docs before changing venue code or arming real money.

Official docs referenced by this chapter:

* [Authentication](https://docs.polymarket.com/api-reference/authentication)
* [CLOB V2 migration](https://docs.polymarket.com/v2-migration)
* [pUSD](https://docs.polymarket.com/concepts/pusd)
* [Create order](https://docs.polymarket.com/trading/orders/create)
* [Matching-engine restarts](https://docs.polymarket.com/trading/matching-engine)
* [User channel](https://docs.polymarket.com/market-data/websocket/user-channel)

## Authentication Model

The official CLOB API uses L1 authentication to create or derive API credentials and L2 authentication headers for authenticated CLOB requests. The TypeScript sidecar uses `@polymarket/clob-client-v2` and `createOrDeriveApiKey()` for this bootstrap.

Current repo defaults target the existing proxy-wallet flow:

* `POLYMARKET_SIGNATURE_TYPE=1`
* `POLYMARKET_FUNDER` defaults to `POLYMARKET_PROXY_WALLET`
* sidecar config comes from `.secrets/buba-paint-live-sidecar.env` locally and remote `~/buba-paint-live/config/sidecar.env`

Official docs now distinguish existing proxy/safe users from newer deposit-wallet API users. New API users may need the deposit-wallet signature/funder model. The current repo default remains proxy-wallet because that is the configured account model; any deposit-wallet migration requires a fresh sidecar/account revalidation before funded trading.

Never expose CLOB API credentials or private keys in the browser. The dashboard must not connect to the Polymarket user channel directly.

## CLOB V2

CLOB V2 is the production contract model used by the sidecar. V2 removed several V1 order assumptions:

* no bot-managed nonce in the signed order model
* no signed `feeRateBps` in new live order logic
* fees are venue metadata and settlement behavior, not a static field embedded by the bot
* order uniqueness and metadata follow the V2 SDK model

The sidecar package currently uses:

* `@polymarket/clob-client-v2`
* `@polymarket/builder-relayer-client`
* `@polymarket/builder-signing-sdk`

Do not reintroduce legacy V1 CLOB or order-utils assumptions.

## pUSD Collateral

Polymarket docs describe pUSD as the collateral token used for trading. It is an ERC-20 on Polygon with 6 decimals and USDC backing enforced onchain.

Repo implications:

* account diagnostics should say pUSD or collateral, not imply USDC.e is still the trading collateral
* account math stays USD-denominated with 6-decimal collateral units where needed
* redemption proceeds are not spendable until account state confirms cash credit
* Bridge withdrawal automation is out of scope unless a future plan adds it

## Order Semantics

The current sidecar write boundary supports immediate market-style orders only:

* FOK: fill entirely immediately or cancel
* FAK: fill available liquidity immediately and cancel the rest

Official order docs define BUY market order amount as dollars and SELL market order amount as shares. The bot/sidecar request model therefore uses `amount_usd` for BUY market orders and `size` for SELL share amount.

Current sidecar policy rejects:

* GTC and GTD resting orders
* post-only orders
* missing market metadata
* unticked prices
* below-min-size orders
* stale auth or account state
* unknown collateral state
* insufficient balance/allowance state

Timeout after submit or unknown venue outcome is dangerous state, not success.

## Matching-Engine Restarts

Polymarket documents HTTP `425` for order-related endpoints during matching-engine restart windows. Treat this as temporary venue restart/degradation. Do not treat it as a successful submission or as a normal permanent rejection.

Runtime implications:

* no new live risk while venue restart state is unresolved
* sidecar and bot logs should preserve the failure classification
* retries must be bounded and backoff-aware
* an armed bot must not remain degraded indefinitely

## User Channel

The authenticated user channel emits private order and trade lifecycle events filtered by API key. It is server-side only and must not be used from dashboard client code.

The sidecar is the right boundary for:

* user-stream connectivity
* order placement, update, and cancellation events
* trade/fill lifecycle events
* sanitized activity recovery
* reconciliation triggers

Live-fidelity validation depends on this private lifecycle evidence plus account snapshots and local decision evidence.

## Market Metadata

Before any funded trading, the deployment host must verify current BTC 5-minute market metadata:

* token IDs
* tick size
* min size
* accepting-order status
* neg-risk fields
* fee metadata
* collateral/account readiness

Gamma discovery can identify BTC 5-minute windows, but CLOB market metadata is the authoritative trading-constraint surface when available. Local assumptions lose to production-safe readonly checks.

## Geoblock And Host Reality

Geoblock checks must run from the actual deployment host. A local laptop pass does not prove the AWS host is allowed, and an AWS pass does not prove a different region or provider is allowed.

Future funded plans must record:

* host geoblock result
* sidecar health
* account/preflight state
* market metadata evidence
* user-stream/activity state
* replay-grade capture health

If Gamma, CLOB, account, or user activity endpoints are blocked from the host, stop and resolve that before considering funded trading.

## Future Funded Posture

Any future first canary should be narrow:

* latency-arb only
* bankroll around `$100`
* calm and spread disabled
* FOK/FAK only
* strict single-order, open-notional, daily-loss, and session-drawdown caps
* terminal halt on auth, geoblock, capture, account, user-stream, reconciliation, or unknown-order failure

This document is not an arming checklist. Use it as venue context for a fresh funded plan.
