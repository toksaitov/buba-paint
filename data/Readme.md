# Data Directory

`data/` contains derived and reproducible artifacts. Primary run evidence lives in `runs/` and must not be edited manually.

Important children:

* `market-data.db`: merged historical research database, ignored by git because it can be rebuilt.
* `backfill-cache/`: API response cache used by backfill and verification workflows.
* `experiments/`: run-specific forensic workspaces, calibration outputs, and scratch replay DBs.
* `sweeps/`: parameter sweep outputs and notes.

Naming rule for new derived work:

* use `run-XXX-topic-NNN`
* keep one topic per directory
* write a short `notes.md`, `postmortem.md`, or `Readme.md` when the directory is not self-explanatory

Legacy names such as `rust-004`, `calm-001`, and `validate-006` are historical. Keep them until their conclusions are migrated or deliberately pruned.
