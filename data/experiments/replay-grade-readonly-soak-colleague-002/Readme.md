# Replay-Grade Readonly Soak: Colleague 002 (90-minute)

Independent no-order `live_readonly` 90-minute soak attempt against `buba-paint-fin` per the former root `PROMPT.md` handoff. Triggered after [colleague-001](../replay-grade-readonly-soak-colleague-001/) (the 5-minute run) passed acceptance.

## Status

In progress. The soak script populates the rest of this directory: `manifest.json`, numbered step logs, `live-preflight.json`, health/process/log/db captures at each phase, replay-quality validation, and the auto-generated `notes.md`.

## Acceptance criteria

Same as the 5-minute run plus:

- No sustained user-stream / account / venue degradation across the 90-minute window.
- Replay-grade required feed classes remain present across the interval.
- No unintended stale process from another release is running.
- Remote runtime is preserved at `/root/buba-paint-live/runtime/soak-001-<stamp>` after closeout.
- Non-secret evidence is copied locally under this directory.
