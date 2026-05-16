# Replay-Grade Readonly Soak 001

Status: auth blocker resolved after initial failed soak; accepted 90-minute soak still pending rerun.

This directory holds local non-secret evidence for the Phase 9 no-order `live_readonly` host soak attempt. Runtime DBs, logs, JSON captures, and text command logs are intentionally ignored by Git. Keep them on disk for local evidence only.

What ran:

* Local readiness gate passed before host mutation. Manifest: `/private/tmp/buba-live-readiness-local-20260502-074056Z/manifest.json`.
* Host layout was bootstrapped under `~/buba-paint-live/config`, `~/buba-paint-live/logs`, `~/buba-paint-live/releases`, and `~/buba-paint-live/runtime`.
* User systemd units were installed for sidecar, bot, agent, and dashboard. User linger is enabled.
* The initial host start attempt failed because the bot refused `live_readonly` startup when sidecar auth, clock, and user-stream checks were unhealthy.
* A patched short probe verified services remain no-order and fail closed, but it still failed authenticated CLOB readiness.

Initial blocking finding:

* Host geoblock check returned unblocked from country `IE`, and the CLOB root endpoint returned `200`.
* CLOB L2 auth bootstrap did not pass from the host.
* Deriving an existing API key returned `Could not derive api key`.
* Creating a new API key hit Cloudflare `403 Forbidden` on `POST https://clob.polymarket.com/auth/api-key`.
* Locally derived nonce-0 CLOB L2 credentials passed repeated balance/open-order reads from the development machine.
* The same locally valid credentials returned `Unauthorized/Invalid api key` from the AWS host.
* Nonzero-nonce L2 credentials were observed to become invalid shortly after creation, so they are not acceptable for a live-readiness soak.
* Standalone host diagnostics reproduced the problem outside the sidecar:
  * Raw Node EIP-712/HMAC script: L1 derive `400`, L1 create Cloudflare `403`, L2 endpoints `401`.
  * Official `py-clob-client-v2`: L1 derive `400`, L1 create Cloudflare `403`, L2 balance/orders/trades `401`.
  * Official `polymarket_client_sdk_v2` Rust SDK: authenticate failed on `GET /auth/derive-api-key` with `400`.
* Official Polymarket authentication docs require L1 create or derive to obtain L2 credentials before authenticated CLOB account, open-order, and user-channel operations. Without that, the sidecar cannot be considered ready.

Follow-up resolution:

* The account completed the Polymarket USDC.e to pUSD migration through the website.
* Fresh nonce-0 CLOB L2 credentials were generated locally from the exported signer key and copied into the stable host sidecar env.
* Standalone raw HMAC probes on the AWS host then passed CLOB `balance-allowance`, `open orders`, and `trades` reads.
* The supervised sidecar on the AWS host then reported `ready=true`, account cash `99.166994`, open venue orders `0`, pUSD collateral diagnostics, active BTC market metadata, and user stream `ok`.
* Host-side `POST /auth/api-key` still hits Cloudflare `403`, so the operating rule for this account is to generate or refresh CLOB L2 credentials locally and then copy them to `~/buba-paint-live/config/sidecar.env` with `0600` permissions before host soaks or canaries.
* The accepted 90-minute no-order soak still needs to be rerun from scratch after this credential refresh.

Safety result:

* No `live_trading` session was started.
* No arming, order placement, cancellation, or redemption was attempted.
* Probe DB quick check returned `ok`.
* Live ledger tables recorded zero order intents, orders, fills, and redemptions.
* Host services were stopped after the attempts.
* Remote failed-attempt logs were scanned after redaction and no unredacted auth-header or env-secret patterns were found.

Fixes made during Phase 9:

* Sidecar SDK console output is now redacted for sensitive auth fields before reaching service logs.
* Sidecar supports preconfigured CLOB L2 credentials through `POLYMARKET_API_KEY`, `POLYMARKET_API_SECRET`, and `POLYMARKET_API_PASSPHRASE`.
* Sidecar auth bootstrap now tries documented derive before create, then reports both failure paths when both fail.
* Host soak automation now treats a failed `live-preflight` response as a hard failure instead of continuing to replay validation.

Next required action:

* Rerun Phase 9 from scratch using the stable refreshed host env. Do not proceed to funded write smoke or canary until the 90-minute no-order soak passes and `live-preflight`, replay-grade validation, DB quick check, process health, and evidence closeout all pass from the deployment host.
