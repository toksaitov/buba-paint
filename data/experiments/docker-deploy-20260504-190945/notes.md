# Docker Deployment Evidence

* Host: `buba-paint`
* Domain: `buba.toksaitov.com`
* Mode: `live-readonly`
* Release: `/home/ubuntu/buba-paint-live/releases/20260504-190945`
* Runtime: `/home/ubuntu/buba-paint-live/runtime/docker-live-readonly-20260504-190945`
* Dashboard user: `admin`
* Dashboard password on host: `/home/ubuntu/buba-paint-live/runtime/docker-live-readonly-20260504-190945/dashboard-admin-password.txt`

Secrets are not copied into this evidence directory.

## Post-Deploy Verification

* Internal Compose stack: sidecar, paint, agent, dashboard, and Caddy started.
* Internal health: sidecar `/health`, agent `/health`, and dashboard `/health` passed.
* SQLite: `/runtime/paint.db` returned `ok` for `PRAGMA quick_check`.
* Mode: paint log shows `execution_mode="live_readonly"` and `feed_event_storage_profile="replay_grade"`.
* Strategies: paint log shows latency only; spread and calm are disabled by deploy env.
* Sidecar: authenticated health is ready, user stream is connected, and account refresh succeeded.
* Live writes: `live_order_intents`, `live_orders`, `live_redemptions`, and `live_control_commands` are all zero.
* Replay gate: `buba-paint validate-replay-data` reported `sweep_grade` for `2026-05-04T19:10:30Z` to `2026-05-04T19:20:00Z`.
* TLS: Caddy obtained a Let's Encrypt certificate for `buba.toksaitov.com`; HTTPS `/health` returns `{"ok":true}`.
* DNS: authoritative, Google, Cloudflare, and local resolution now return `34.248.168.57`.
* Public edge: `http://buba.toksaitov.com` returns a Caddy `308` redirect to HTTPS, and TCP `80`/`443` are reachable.
