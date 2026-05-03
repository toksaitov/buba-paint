# Replay-Grade Readonly Soak 003

Status: failed before an accepted 90-minute soak.

This directory contains the clean attached Phase 9 host-soak runner attempt from 2026-05-02. The runner staged and built release `phase9-readonly-soak-20260502-125110Z`, started the sidecar, and failed at the preflight gate.

What happened:

- The runner failed `live-preflight` because authenticated CLOB account reads returned `Unauthorized/Invalid api key`.
- After refreshing nonce-0 CLOB L2 credentials locally and copying them to the host, a manual restart passed initial preflight and started sidecar, bot, agent, and dashboard in `live_readonly`.
- The 5-minute check showed all four services active and the bot collecting market/feed data, but authenticated CLOB reads degraded again.
- Raw host diagnostics at the 5-minute check showed signer-address `/data/trades` still worked, while signer-address `balance-allowance` and `/data/orders` returned `401`.
- Funder-address authenticated reads returned `401`.
- Host services were stopped after the failed 5-minute auth-stability check.

Safety result:

- No `live_trading` session was started.
- No arming, order placement, cancellation, redemption, or funded write smoke occurred.
- DB quick check returned `ok`.
- The live ledger stayed empty for order intents, orders, fills, and redemptions.

Conclusion:

The host deployment layout is usable, but authenticated CLOB L2 credentials are not stable enough for a 90-minute readonly soak. Phase 9 remains blocked until the deployment host can obtain and keep durable authenticated CLOB credentials, or the deployment is moved to a network/account configuration where Polymarket accepts the documented auth flow for sustained account/open-order reads.
