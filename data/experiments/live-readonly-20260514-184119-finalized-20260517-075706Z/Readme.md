# Live Readonly Finalized Artifact

This directory contains the finalized local copy of the
`live-readonly-20260514-184119` runtime from `buba-paint`.

The run was frozen on 2026-05-17 by stopping the remote `paint` and `sidecar`
services, checkpointing SQLite WAL state, and verifying `PRAGMA quick_check`.
The original runtime DB remains on `buba-paint`; this copy is the local
research artifact used for worker smoke testing.

Do not delete this directory unless the finalized run artifact has been moved
to another durable location and the user explicitly approves the cleanup.
