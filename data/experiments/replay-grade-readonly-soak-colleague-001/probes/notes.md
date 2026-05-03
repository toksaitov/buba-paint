# CLOB v2 Auth Probe Runs

Independent authenticated CLOB v2 reads against `https://clob.polymarket.com` outside the sidecar code, per the [PROMPT.md "Independent Auth Probe Requirement"](../../../../PROMPT.md). The probe script lives at [polymarket-sidecar/scripts/clob-auth-probe.mjs](../../../../polymarket-sidecar/scripts/clob-auth-probe.mjs); it depends only on `@ethersproject/wallet`, `node:https`, and `node:crypto`.

## Coverage

Each probe run exercises:

- L1 EIP-712 typed-data signing (`ClobAuthDomain` v1, chainId 137, `ClobAuth(address,timestamp,nonce,message)`, message `This message attests that I control the given wallet`).
- L2 HMAC-SHA256 signing (`base64_decode(secret)` key, message `${ts}${method}${requestPath}${body}`, base64-then-URL-safe).
- `GET /time`.
- `GET /balance-allowance` over both signature_type 1 (POLY_PROXY) and signature_type 2 (POLY_GNOSIS_SAFE) variants.
- `GET /data/orders` (current open-order endpoint on CLOB v2).
- `POST /auth/derive-api-key` over a small nonce scan and (when `--force-create`) `POST /auth/api-key`.
- A stability loop of N authenticated balance + open-order reads.

Output JSON is redacted: signer/proxy/funder addresses, signatures, secrets, passphrases, and private keys are replaced with explicit placeholder strings before the file is written.

## Run table

| Run | When | Iter | Bal pass | Ord pass | Avg bal ms | Avg ord ms | P95 bal ms | P95 ord ms | Sig types working | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| `local-001` | 2026-05-03 05:24Z | 25 | 0 | 0 | n/a | n/a | n/a | n/a | none | Pre-create-attempt, reproduced May-2 401 blocker |
| `local-002` | 2026-05-03 05:25Z | 25 | 25 | 25 | 143 | 153 | 214 | 218 | 1, 2 | Ran with `--force-create`; preconfigured creds began passing |
| `local-003` | 2026-05-03 05:25Z | 30 | 30 | 30 | 140 | 131 | 214 | 143 | 1, 2 | Stability check on macOS |
| `local-004` | 2026-05-03 05:26Z | 30 | 30 | 30 | 136 | 139 | 152 | 214 | 1, 2 | Stability check on macOS |
| `buba-paint-ireland-001` | 2026-05-03 05:33Z | 25 | 25 | 25 | 30 | 31 | 33 | 34 | 1, 2 | Ran on `ssh buba-paint` (Ireland AWS) |
| `buba-paint-ireland-002` | 2026-05-03 05:35Z | 60 | 60 | 60 | 31 | 31 | 46 | 41 | 1, 2 | Longer stability run on Ireland host |
| `buba-paint-fin-001` | 2026-05-03 05:41Z | 25 | 25 | 25 | 102 | 104 | n/a | n/a | 1, 2 | Ran on `ssh buba-paint-fin` (Finland Hetzner), the soak target |
| `buba-paint-fin-002` | 2026-05-03 05:41Z | 60 | 60 | 60 | 102 | 102 | n/a | n/a | 1, 2 | Longer stability run on Finland host |

The local probe directory is at `/tmp/buba-clob-probe/` and is intentionally not committed back into this directory. The four host probe artifacts live here.

Total successful authenticated CLOB v2 reads since the May-2 blocker resolved: 255 balance + 255 open-order reads, both signature_type 1 and 2 passing, zero failures, across macOS, Ireland AWS, and Finland Hetzner.

## Inferences

- The Phase 9 May-2 "401 Unauthorized/Invalid api key" plus `deriveApiKey` empty-recovery plus `createApiKey` Cloudflare-block pattern is no longer reproducible from either macOS or the Ireland host using the same env values.
- The preconfigured `POLYMARKET_API_KEY` / `_SECRET` / `_PASSPHRASE` are valid CLOB v2 L2 credentials. `chosen_creds_source=preconfigured` in every passing run.
- `walletAddress = signer` (`POLY_ADDRESS`), not the proxy, matches the May-2 finding and the SDK behavior.
- Latency is dominated by network distance: macOS ~135 ms p50, Ireland ~30 ms p50.

The Finland host probe is now in place. The auth path is no longer the soak blocker. Next gate is the deployment dry-run and the 5-minute readonly soak.
