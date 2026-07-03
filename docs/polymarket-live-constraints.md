# Polymarket Live Constraints

These are current-truth operational constraints for live trading, with the place each is enforced in code or config called out inline. They are load-bearing yet easy to get wrong and are not obvious from the code. Several are unstable by nature, so re-check official Polymarket docs before changing venue code or arming real money.

Official docs referenced by this chapter:

* [Authentication](https://docs.polymarket.com/api-reference/authentication)
* [CLOB V2 migration](https://docs.polymarket.com/v2-migration)
* [pUSD](https://docs.polymarket.com/concepts/pusd)
* [Create order](https://docs.polymarket.com/trading/orders/create)
* [Fees](https://docs.polymarket.com/trading/fees)
* [Matching-engine restarts](https://docs.polymarket.com/trading/matching-engine)
* [User channel](https://docs.polymarket.com/market-data/websocket/user-channel)

## Jurisdiction And Geoblock Reality

The operator is based in Kyrgyzstan. The live host runs on AWS `eu-west-1` (Ireland). Ireland was chosen for network latency to Polymarket (about 30 ms p50 from Ireland versus about 135 ms from a residential path), not as a geo workaround. Kyrgyzstan is not on Polymarket's US or sanctioned blocklist, so this is latency optimization, not geoblock circumvention.

Polymarket's geoblock is egress-based: it inspects the connecting IP, not the operator's residence. From the Ireland host, live preflight returns `blocked=false` with `country=IE`, which is a normal allowed region. This is a directly connected hosted AWS instance, not a VPN, so the egress is a genuine AWS Ireland datacenter IP. This topology is settled and should not be re-litigated. Authoritative host evidence lives under `data/experiments/replay-grade-readonly-soak-001` through `data/experiments/replay-grade-readonly-soak-003` (observed `blocked=false`, `country=IE`, geoblock status ok, with hundreds of successful authenticated CLOB V2 reads and zero failures).

The pinned egress IP must still be read live at canary time. AWS egress IPs can reassign, so a previously observed address is not durable. Read the server's current egress at arm time and pin it into `LIVE_EXPECTED_EGRESS_IP` rather than assuming a fixed value.

Geoblock and readiness checks must run from the actual deployment host. A local laptop pass does not prove the AWS host is allowed, and an AWS pass does not prove a different region or provider is allowed. Any funded soak or canary must record host geoblock result, sidecar health, account and preflight state, market metadata evidence, user-stream and activity state, and replay-grade capture health. If Gamma, CLOB, account, or user activity endpoints are blocked from the host, stop and resolve that before considering funded trading.

Enforced in code and config:

* `bots/paint/src/config.rs` validates `LIVE_EXPECTED_EGRESS_IP` as a real IP address.
* `egress_ip_issues` in `bots/paint/src/live.rs` blocks arming when the pinned egress IP does not match, or cannot be confirmed against, the observed geoblock IP.
* The canary egress-pin step is described in [deployment-and-ops.md](./deployment-and-ops.md) and [canary-config.md](./canary-config.md).

## Settlement Oracle

Polymarket resolves BTC 5-minute Up/Down markets on Chainlink, not on Binance. Any provisional Binance-derived settlement the runtime computes is observability only. It has historically disagreed with the real outcome about a third of the time. Run 008 is the reference example: a roughly 74% local win rate sat against a roughly 50% real win rate on the same trades, because provisional Binance settlements captured at imprecise moments diverged from the Chainlink outcome at the true window boundary.

Because of that gap, bankroll and PnL must update only on the authoritative Gamma resolution, never on the provisional Binance signal. Any run's parameters must be verified against Polymarket's actual outcomes before they are taken seriously.

Enforced in code and config:

* `run_verify_settlements` in `bots/paint/src/verify.rs` (the `verify-settlements` CLI) fetches authoritative resolutions from the Gamma API and audits paper or shadow PnL against them.
* Settled outcomes come from `markets.outcome`, which the backtester treats as settlement truth and refuses to guess when missing. See the Backtest Semantics section of [data-and-replay.md](./data-and-replay.md) for how replay handles settlement and outcomes.

## Venue Timing

Authoritative resolution lands roughly 40 seconds to 4 or more minutes after a window closes. The runtime must never block inline waiting for it. Resolution is fetched by a bounded retry worker, and any new gate that waits for resolution inside the decision path will stall the runtime.

Separately, `market_discovery` surfaces the next slot about 3 to 5 minutes before its `start_time`. A discovered window is a future window, not a tradable one. It must only be activated as the current tradable window after its `start_time` has passed. Firing against a window that has not started yet books trades into the wrong window with the wrong settlement boundary and destroys PnL attribution.

Enforced in code and config:

* `bots/paint/src/market_discovery.rs` derives the next slot ahead of time and tracks known windows; activation of the current window is deferred until start time.
* The bounded settlement and resolution worker, and the rule that the hot path must not await settlement fetches, are described in the Hot Path sections of [system-architecture.md](./system-architecture.md) and [strategy-and-risk.md](./strategy-and-risk.md).

## Order-Book Liquidity

Real depth on these 5-minute markets is thin and highly variable. The median USD available at the best ask is about 498 dollars. The mean is far higher (about 8,476 dollars) because the distribution is heavily right-skewed, so the mean is not a safe planning number. Depth also varies roughly 10x within a single window: it is thin at window open, peaks near the 1:30 to 2:00 mark as market makers post, then thins again before settlement as they pull liquidity.

Sizing and backtests must model fills against the actual `ask_size` and clamp orders to book depth. Without that clamp, sweeps invent fictional multi-thousand-dollar fills and produce impossible PnL. The `$500` default hard cap is the safety rail behind realistic depth-aware sizing.

Enforced in code and config:

* `MAX_POSITION_USD` defaults to `500.0` in `bots/paint/src/config.rs` as the per-position hard cap.
* Replay-grade capture preserves CLOB best bid, best ask, bid size, and ask size so fills can be modeled against real depth. See the CLOB Replay Blocks section of [data-and-replay.md](./data-and-replay.md).
* Bankroll, exposure, and per-family sizing controls are described in the Bankroll And Exposure section of [strategy-and-risk.md](./strategy-and-risk.md).

## Fees

Since CLOB V2, fees apply at match time and are read live per market from the fee details on the market metadata (`fd.r` for the rate and `fd.e` for the exponent). The client no longer sets a fee rate on the order. The current crypto taker rate is `0.07` with exponent `1`.

The per-share fee model is:

```
fee_per_share = price * feeRate * (price * (1 - price))^exponent
```

At an entry near `$0.50`, where the latency strategy tends to enter, this fee peaks.

Enforced in code and config:

* `bots/paint/src/fees.rs` resolves fee params (live market schedule first, then crypto defaults) and computes the fee with `compute_taker_fee`, which is the per-share model above multiplied by shares.
* `bots/paint/src/config.rs` defaults `taker_fee_rate` to `0.07` and `taker_fee_exponent` to `1`. Any change to that constant is a numeric-sensitive operator decision.
* The sidecar preflight fee-mismatch gate compares the live per-market rate (`fd.r`) against `POLYMARKET_EXPECTED_TAKER_FEE_RATE` (default `0.07`) and blocks arming when the observed rate exceeds it by more than `0.02`. The operational detail and the rest of the fee knobs live in [commands-and-config.md](./commands-and-config.md).

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

## Future Funded Posture

Any future first canary should be narrow:

* latency-arb only
* bankroll around `$100`
* calm and spread disabled
* FOK/FAK only
* strict single-order, open-notional, daily-loss, and session-drawdown caps
* terminal halt on auth, geoblock, capture, account, user-stream, reconciliation, or unknown-order failure

This document is not an arming checklist. Use it as venue context for a fresh funded plan.
