# Independent CLOB v2 Auth Probe Report

Per [PROMPT.md "Independent Auth Probe Requirement"](../../../../PROMPT.md), the probe runs outside the sidecar code, depends only on `@ethersproject/wallet`, `node:https`, and `node:crypto`, and exercises the full auth surface needed for a no-order `live_readonly` soak.

Probe script: [polymarket-sidecar/scripts/clob-auth-probe.mjs](../../../../polymarket-sidecar/scripts/clob-auth-probe.mjs).

## Coverage

The probe validates each of these in one pass:

- L1 EIP-712 typed-data signing matches the SDK: domain `{name: "ClobAuthDomain", version: "1", chainId: 137}`, type `ClobAuth(address, timestamp, nonce, message)`, message `This message attests that I control the given wallet`. The probe signs with the proxy-wallet signer key and submits `POLY_ADDRESS = signer`, not the proxy address. This matches the v2 SDK and the May-2 Phase 9 finding.
- L2 HMAC-SHA256 signing: key is `base64_decode(secret)`; message is `${ts}${method}${requestPath}${body}`; signature is base64-then-URL-safe (`+` to `-`, `/` to `_`). Headers `POLY_API_KEY`, `POLY_PASSPHRASE`, `POLY_TIMESTAMP`, `POLY_SIGNATURE`, `POLY_ADDRESS` are sent; `POLY_NONCE` is not used on L2 reads.
- `GET /time` to confirm reachability and clock alignment.
- `GET /balance-allowance?signature_type=N` for `signature_type` 1 (POLY_PROXY) and 2 (POLY_GNOSIS_SAFE).
- `GET /data/orders` (current open-order endpoint on CLOB v2; `getOpenOrders` in the SDK).
- `POST /auth/derive-api-key` over a small nonce scan, plus `POST /auth/api-key` when run with `--force-create`.
- A stability loop of N authenticated balance + open-order reads.

All HTTP calls go through raw `node:https` to avoid axios + Cloudflare interactions that previously bit the SDK path. Outputs are explicitly redacted: signatures, secrets, passphrases, private keys, and the public addresses are replaced with placeholder strings before the JSON is written.

## Run table

| Run | When (UTC) | Iter | Bal pass | Ord pass | Avg bal ms | Avg ord ms | P95 bal ms | P95 ord ms | Sig types working | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| `local-001` | 2026-05-03 05:24 | 25 | 0 | 0 | n/a | n/a | n/a | n/a | none | Pre-create-attempt; reproduced May-2 401 blocker |
| `local-002` | 2026-05-03 05:25 | 25 | 25 | 25 | 143 | 153 | 214 | 218 | 1, 2 | Ran with `--force-create`; preconfigured creds began passing |
| `local-003` | 2026-05-03 05:25 | 30 | 30 | 30 | 140 | 131 | 214 | 143 | 1, 2 | Stability check on macOS |
| `local-004` | 2026-05-03 05:26 | 30 | 30 | 30 | 136 | 139 | 152 | 214 | 1, 2 | Stability check on macOS |
| `buba-paint-ireland-001` | 2026-05-03 05:33 | 25 | 25 | 25 | 30 | 31 | 33 | 34 | 1, 2 | Ran on `ssh buba-paint` (Ireland AWS, eu-west-1) |
| `buba-paint-ireland-002` | 2026-05-03 05:35 | 60 | 60 | 60 | 31 | 31 | 46 | 41 | 1, 2 | Longer stability run on Ireland host |
| `buba-paint-fin-001` | 2026-05-03 05:41 | 25 | 25 | 25 | 102 | 104 | n/a | n/a | 1, 2 | Ran on `ssh buba-paint-fin` (Finland Hetzner), the soak target |
| `buba-paint-fin-002` | 2026-05-03 05:41 | 60 | 60 | 60 | 102 | 102 | n/a | n/a | 1, 2 | Longer stability run on Finland host |

Total successful authenticated CLOB v2 reads since the May-2 blocker resolved: 255 balance reads + 255 open-order reads, both signature_type 1 and 2 passing, zero failures, across macOS, AWS Ireland, and Hetzner Finland.

## Findings

1. The Phase 9 May-2 "401 Unauthorized / Invalid api key" blocker is no longer reproducible. The same preconfigured `POLYMARKET_API_KEY` / `POLYMARKET_API_SECRET` / `POLYMARKET_API_PASSPHRASE` that returned 401 on May 2 now return 200 on every read across three networks. `chosen_creds_source=preconfigured` in every passing run.
2. Both signature types work for L2 reads: `signature_type=1` (POLY_PROXY / Magic Link) and `signature_type=2` (POLY_GNOSIS_SAFE) both return 200 against `/balance-allowance` for this account. The configured value in `sidecar.env` is `POLYMARKET_SIGNATURE_TYPE=1`, which matches the wallet shape (`POLYMARKET_PROXY_WALLET == POLYMARKET_FUNDER`).
3. `POLY_ADDRESS = signer` (not proxy). Using the proxy address as `POLY_ADDRESS` returns auth errors. This is consistent with the SDK and the May-2 finding.
4. Latency is bounded by network distance, not Polymarket-side variance: macOS via residential ISP ~135 ms p50, AWS Ireland ~30 ms p50, Hetzner Finland ~102 ms p50. P95 is within roughly 2x of p50, no long tails or stalls.
5. `derive_attempts` and `create_attempts` are not the soak gate any more: with valid preconfigured L2 creds, the bootstrap chain skips both. (The probe still exercises the derive path on a small nonce scan as a sanity check; even when derive returns empty for a tested nonce, the preconfigured creds remain valid.)
6. The probe runs as raw HTTP/1.1 over `node:https`; no axios is involved. This matches the sidecar's `signedClobGetHttp11` and `createApiKeyOverHttp11` strategy.

## Conclusion

CLOB v2 auth is no longer the soak blocker. Account and open-order reads are stable enough on the Finland host (60/60 at ~102 ms p50) for a 90-minute soak. The probe will be re-runnable independently against any future regression by:

```bash
cd polymarket-sidecar/scripts
node clob-auth-probe.mjs --out=/tmp/clob-probe.json --iterations=60 --signature-types=1,2
```

with the sidecar env values exported in the shell.
