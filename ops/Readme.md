# Operations Artifacts

`ops/` contains deployment templates and runbook helpers for the `buba-paint` host. Docker Compose with Caddy is the preferred remote deployment model. The systemd files are retained as legacy/reference templates, not automatically installed services.

Target host layout:

* active release: `~/buba-paint-live/current`
* runtime DBs and run logs: `~/buba-paint-live/runtime/<runtime-name>`
* stable config files: `~/buba-paint-live/config`
* Caddy state: `~/buba-paint-live/caddy`

Docker deployment notes live in [docker/Readme.md](./docker/Readme.md).

Systemd user-service templates live in `ops/systemd/`.

* `buba-polymarket-sidecar.service`: supervised authenticated Polymarket sidecar
* `buba-paint-bot.service`: supervised bot process template
* `buba-agent.service`: monitor-only agent template
* `buba-dashboard.service`: dashboard server template

Docker Compose is preferred for normal readonly/live-readiness operation. The systemd templates are reference material for future operators who deliberately choose a non-Docker process model. Before installing any template, copy the matching env file into `~/buba-paint-live/config`, verify paths, and run the local validation gates listed in `CLAUDE.md`.

The service templates read runtime-specific log paths from env files:

* `BUBA_SIDECAR_LOG_PATH`
* `BUBA_PAINT_LOG_PATH`
* `BUBA_AGENT_LOG_PATH`
* `BUBA_DASHBOARD_LOG_PATH`

If these are absent, services fall back to `~/buba-paint-live/logs/*.log`. For host readonly soaks and any future funded run, prefer runtime-specific paths so evidence stays with the run directory.
