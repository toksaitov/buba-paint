# Operations Artifacts

`ops/` contains deployment templates and runbook helpers for the `buba-paint` host. These files are source-controlled references, not automatically installed services.

Target host layout:

- active release: `~/buba-paint-live/current`
- runtime DBs and run logs: `~/buba-paint-live/runtime/run-0NN`
- stable config files: `~/buba-paint-live/config`
- stable process logs: `~/buba-paint-live/logs`

Systemd user-service templates live in `ops/systemd/`.

- `buba-polymarket-sidecar.service`: supervised authenticated Polymarket sidecar
- `buba-paint-bot.service`: supervised bot process template
- `buba-agent.service`: monitor-only agent template
- `buba-dashboard.service`: dashboard server template

The sidecar should be supervised for normal readonly/live-readiness operation. Bot, agent, and dashboard service templates are provided so future operators can move away from ad hoc shells without inventing a new layout. Before installing any template, copy the matching env file into `~/buba-paint-live/config`, verify paths, and run the local validation gates listed in `CLAUDE.md`.
