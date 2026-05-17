# Remote Runtime Copy

This directory contains the copied runtime files from the finalized
`buba-paint` live-readonly run.

Important files:

* `paint.db`: finalized SQLite DB after WAL checkpoint.
* `paint.db-wal`: retained sidecar, expected to be zero bytes after checkpoint.
* `paint.db-shm`: retained sidecar for evidence completeness.
* `paint.log`, `sidecar.log`, `agent.log`, `dashboard.log`: runtime logs.
* `dashboard.db`: dashboard runtime DB captured with the evidence bundle.

The DB is used read-only for research-worker smoke tests and later backtesting.
