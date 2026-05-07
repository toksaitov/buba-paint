# Run 013 Docker live-readonly incident notes

Remote runtime: `/home/ubuntu/buba-paint-live/runtime/docker-live-readonly-20260504-235600`

Local archive target: `runs/013/server-20260507-210428/`

Observed before stopping:

- Docker containers were still running and sidecar health was ready.
- Sidecar account/user-stream state was current and healthy.
- Bot public feed and strategy/window pipeline were degraded.
- `feed_events` effectively stopped growing after `2026-05-07 15:49 UTC`.
- Bot logs continued emitting storage/rejection rollups, but mostly against old markets with `window_too_late`.
- Bot process was in uninterruptible disk wait: state `Dl`, wait channel `folio_wait_bit_common`.
- Host `vmstat` showed sustained high I/O wait, often around `50-90%`.
- Paint process I/O counters showed roughly `4.28 TB` read and `768 GB` written over the run.
- Likely root cause: periodic storage/replay-quality reporting performs repeated full scans over the large `feed_events` table. On the small AWS disk this starved the runtime, delayed reconnect/window timers, and made the data after the stall unreliable.

Important code paths to inspect:

- `bots/paint/src/live.rs`: storage report timer calls `db.storage_footprint()` and `persist_replay_quality_metadata()`.
- `bots/paint/src/db/database.rs`: `storage_footprint()` runs full `feed_events` counts/grouping.
- `bots/paint/src/backtest/replay_quality.rs`: replay-quality validation runs multiple interval counts over `feed_events`.

Do not treat this archive as a clean 72-hour research run without cutting or annotating the degraded interval.
