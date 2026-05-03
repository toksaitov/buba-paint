# Replay-Grade Readonly Soak 004

Manual no-order `live_readonly` soak attempt on `buba-paint-fin` after local CLOB L2 credential probing and the sidecar user-stream heartbeat patch. Scope: no `live_trading`, no arming, no orders, no cancels, no redemptions.

## Result

Failed acceptance after a short `live_readonly` run. The release and runtime layout were staged successfully, Rust binaries built on the Finland host, user-systemd linger was enabled, and the supervised sidecar, bot, agent, and dashboard all started. The bot stayed in `live_readonly`; no live order path was enabled.

The sidecar user stream no longer shows the old idle-disconnect failure when it has a stable subscription; the heartbeat patch keeps the authenticated stream alive in the simple stability check. During the runtime, user-stream reconnects still occurred around market subscription changes, but account truth was the acceptance blocker.

The runtime DB is intact and replay-grade for public feed capture:

- SQLite `PRAGMA quick_check` returned `ok`.
- `validate-replay-data` returned `replay_quality=sweep_grade` for `2026-05-02T16:12:33Z` to `2026-05-02T16:30:11Z`.
- Required rows were present: Binance `aggTrade`, Binance `bookTicker`, Binance `depth`, Chainlink price, CLOB UP top of book, and CLOB DOWN top of book.
- Live money tables remained empty: `live_order_intents=0`, `live_orders=0`, `live_fills=0`, and `live_redemptions=0`.

The accepted 90-minute soak remains blocked because authenticated CLOB L2 account reads are not durable:

- Minimal raw HTTP/1.1 signed probes outside the sidecar reproduce intermittent `401 Unauthorized/Invalid api key` locally and on the Finland host with the same configured CLOB L2 credentials.
- The signer address is the correct `POLY_ADDRESS`; using the proxy address fails consistently.
- Existing-key derivation did not recover a fresh key for tested nonces `0..100`.
- `createApiKey` remains Cloudflare-blocked from non-browser runtimes, including local and Finland probes.
- Relayer API keys from the website are not enough for CLOB account reads because CLOB L2 auth requires `apiKey`, `secret`, and `passphrase`.

All host processes were stopped after the failed acceptance check. The remote runtime is preserved at `/root/buba-paint-live/runtime/soak-004-20260502-152805Z`; DB files were not copied back into the repository.
