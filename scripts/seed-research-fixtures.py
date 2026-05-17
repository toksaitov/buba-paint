#!/usr/bin/env python3
"""Seed dashboard research fixture data for UI and QA work.

The script is intentionally opt-in. It only deletes and recreates rows whose
IDs start with `fixture-`, and writes files under the supplied work root.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sqlite3
import sys
import time
from pathlib import Path


SCHEMA = """
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'observer' CHECK(role IN ('admin','observer')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    token TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS research_machines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('live','research','controller')),
    ssh_alias TEXT,
    status TEXT NOT NULL,
    details_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS run_artifacts (
    id TEXT PRIMARY KEY,
    source_machine_id TEXT REFERENCES research_machines(id),
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    run_mode TEXT,
    artifact_root TEXT,
    manifest_path TEXT,
    bundle_path TEXT,
    source_db_path TEXT,
    interval_start_ms INTEGER,
    interval_end_ms INTEGER,
    bytes INTEGER,
    checksum TEXT,
    replay_quality_class TEXT,
    backtest_ready_class TEXT,
    live_fidelity_class TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER
);
CREATE TABLE IF NOT EXISTS artifact_transfers (
    id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL REFERENCES run_artifacts(id),
    source_machine_id TEXT REFERENCES research_machines(id),
    dest_machine_id TEXT REFERENCES research_machines(id),
    status TEXT NOT NULL,
    bytes_total INTEGER,
    bytes_done INTEGER NOT NULL DEFAULT 0,
    checksum_status TEXT,
    error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER
);
CREATE TABLE IF NOT EXISTS research_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL CHECK(job_type IN ('export','current_params','sweep')),
    artifact_id TEXT REFERENCES run_artifacts(id),
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    requested_by TEXT NOT NULL REFERENCES users(id),
    params_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    cancelled_at INTEGER,
    completed_at INTEGER
);
CREATE TABLE IF NOT EXISTS research_job_steps (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES research_jobs(id),
    step_index INTEGER NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    lease_owner TEXT,
    leased_until_ms INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    input_json TEXT,
    output_json TEXT,
    error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    UNIQUE(job_id, step_index)
);
CREATE TABLE IF NOT EXISTS research_job_events (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES research_jobs(id),
    step_id TEXT REFERENCES research_job_steps(id),
    timestamp_ms INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    details_json TEXT
);
CREATE TABLE IF NOT EXISTS research_reports (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES research_jobs(id),
    artifact_id TEXT REFERENCES run_artifacts(id),
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    summary_json TEXT,
    report_path TEXT,
    csv_path TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"""


STEP_TEMPLATES = {
    "export": [
        "plan_export",
        "snapshot_or_copy_runtime",
        "write_artifact_manifest",
        "verify_artifact",
    ],
    "current_params": [
        "verify_artifact",
        "validate_replay_data",
        "validate_backtest_input",
        "prepare_backtest_input",
        "run_backtest",
        "write_report",
    ],
    "sweep": [
        "verify_artifact",
        "validate_replay_data",
        "validate_backtest_input",
        "prepare_backtest_input",
        "run_sweep",
        "write_report",
    ],
}


def parse_args() -> argparse.Namespace:
    """Parse command-line options."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", required=True, help="Dashboard SQLite DB to seed.")
    parser.add_argument(
        "--work-root",
        required=True,
        help="Research work root where fixture files will be written.",
    )
    parser.add_argument(
        "--reset",
        action="store_true",
        help="Remove existing fixture-* rows/files before seeding.",
    )
    return parser.parse_args()


def now_ms() -> int:
    """Return the current Unix time in milliseconds."""
    return int(time.time() * 1000)


def json_text(value: object) -> str:
    """Serialize compact deterministic JSON."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def sha256_bytes(data: bytes) -> str:
    """Return a lowercase SHA-256 digest for bytes."""
    return hashlib.sha256(data).hexdigest()


def reset_fixture_rows(conn: sqlite3.Connection, work_root: Path) -> None:
    """Delete existing fixture rows and fixture file directories."""
    conn.execute("DELETE FROM sessions WHERE id LIKE 'fixture-%' OR user_id LIKE 'fixture-%'")
    for table, column in [
        ("research_reports", "id"),
        ("research_job_events", "id"),
        ("research_job_steps", "id"),
        ("research_jobs", "id"),
        ("artifact_transfers", "id"),
        ("run_artifacts", "id"),
        ("research_machines", "id"),
        ("users", "id"),
    ]:
        conn.execute(f"DELETE FROM {table} WHERE {column} LIKE 'fixture-%'")
    for child in [work_root / "artifacts", work_root / "jobs"]:
        if child.exists():
            for path in child.glob("fixture-*"):
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()


def fixture_rows_exist(conn: sqlite3.Connection) -> bool:
    """Return true when the database already contains fixture rows."""
    checks = [
        ("sessions", "id"),
        ("users", "id"),
        ("research_machines", "id"),
        ("run_artifacts", "id"),
        ("artifact_transfers", "id"),
        ("research_jobs", "id"),
        ("research_job_steps", "id"),
        ("research_job_events", "id"),
        ("research_reports", "id"),
    ]
    return any(
        conn.execute(
            f"SELECT 1 FROM {table} WHERE {column} LIKE 'fixture-%' LIMIT 1"
        ).fetchone()
        is not None
        for table, column in checks
    )


def ensure_fixture_user(conn: sqlite3.Connection, timestamp_ms: int) -> None:
    """Insert the fixture requester user."""
    conn.execute(
        """
        INSERT INTO users (id, username, password, role, created_at, updated_at)
        VALUES (?, ?, ?, 'admin', ?, ?)
        ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at
        """,
        (
            "fixture-user-researcher",
            "fixture-researcher",
            "fixture-password-hash",
            timestamp_ms,
            timestamp_ms,
        ),
    )


def insert_machines(conn: sqlite3.Connection, timestamp_ms: int) -> None:
    """Insert fixture machine rows."""
    machines = [
        (
            "fixture-live",
            "Fixture Live Source",
            "live",
            "fixture-live",
            "configured",
            {"host": "fixture-live", "purpose": "ui-source"},
        ),
        (
            "fixture-research",
            "Fixture Research Worker",
            "research",
            "fixture-research",
            "idle",
            {
                "worker_id": "fixture-worker",
                "heartbeat_status": "idle",
                "queue_depth": 0,
            },
        ),
        (
            "fixture-disabled",
            "Fixture Disabled Worker",
            "research",
            "fixture-disabled",
            "disabled",
            {"reason": "operator disabled"},
        ),
    ]
    for machine in machines:
        conn.execute(
            """
            INSERT INTO research_machines (
                id, name, role, ssh_alias, status, details_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (*machine[:5], json_text(machine[5]), timestamp_ms, timestamp_ms),
        )


def write_artifact(
    work_root: Path, artifact_id: str, timestamp_ms: int, corrupt: bool
) -> dict:
    """Write one manifest-backed fixture artifact and return DB metadata."""
    artifact_root = work_root / "artifacts" / artifact_id
    artifact_root.mkdir(parents=True, exist_ok=True)
    payload = f"{artifact_id}: runtime fixture database\n".encode()
    db_path = artifact_root / "paint.db"
    db_path.write_bytes(payload)
    digest = sha256_bytes(payload)
    manifest = {
        "schema_version": 1,
        "artifact_id": artifact_id,
        "kind": "readonly_run",
        "source_machine_id": "fixture-live",
        "run_mode": "live_readonly",
        "created_at_ms": timestamp_ms,
        "interval_start_ms": 1_779_000_000_000,
        "interval_end_ms": 1_779_000_600_000,
        "files": [
            {
                "logical_name": "runtime_db",
                "kind": "sqlite",
                "relative_path": "paint.db",
                "bytes": len(payload),
                "sha256": digest,
            }
        ],
    }
    (artifact_root / "manifest.json").write_text(
        json.dumps(manifest, indent=2), encoding="utf-8"
    )
    (artifact_root / "checksums.sha256").write_text(
        f"{digest}  paint.db\n", encoding="utf-8"
    )
    if corrupt:
        db_path.write_bytes(payload + b"corrupted-after-manifest\n")
    return {
        "root": str(artifact_root),
        "manifest": str(artifact_root / "manifest.json"),
        "bytes": len(payload),
        "checksum": digest,
        "source_db_path": str(db_path),
    }


def insert_artifacts(conn: sqlite3.Connection, work_root: Path, timestamp_ms: int) -> None:
    """Insert fixture artifact rows and sidecar files."""
    artifacts = [
        ("fixture-artifact-available", "available", None, False),
        ("fixture-artifact-archived", "archived", timestamp_ms, False),
        ("fixture-artifact-bad-checksum", "available", None, True),
    ]
    for artifact_id, status, archived_at, corrupt in artifacts:
        meta = write_artifact(work_root, artifact_id, timestamp_ms, corrupt)
        conn.execute(
            """
            INSERT INTO run_artifacts (
                id, source_machine_id, kind, status, run_mode, artifact_root,
                manifest_path, bundle_path, source_db_path, interval_start_ms,
                interval_end_ms, bytes, checksum, replay_quality_class,
                backtest_ready_class, live_fidelity_class, created_at, updated_at, archived_at
            ) VALUES (?, 'fixture-live', 'readonly_run', ?, 'live_readonly', ?, ?, NULL, ?,
                      1779000000000, 1779000600000, ?, ?, 'sweep_grade',
                      'backtest_ready', 'not_checked', ?, ?, ?)
            """,
            (
                artifact_id,
                status,
                meta["root"],
                meta["manifest"],
                meta["source_db_path"],
                meta["bytes"],
                meta["checksum"],
                timestamp_ms,
                timestamp_ms,
                archived_at,
            ),
        )


def insert_transfers(conn: sqlite3.Connection, timestamp_ms: int) -> None:
    """Insert fixture transfer lifecycle rows."""
    transfers = [
        ("fixture-transfer-running", "running", 1_000, 400, "pending", None, None),
        (
            "fixture-transfer-retryable",
            "retryable",
            1_000,
            700,
            "failed",
            "network reset",
            None,
        ),
        (
            "fixture-transfer-paused",
            "paused",
            1_000,
            250,
            "pending",
            None,
            None,
        ),
        (
            "fixture-transfer-completed",
            "completed",
            1_000,
            1_000,
            "verified",
            None,
            timestamp_ms,
        ),
    ]
    for transfer in transfers:
        transfer_id, status, total, done, checksum, error, completed = transfer
        conn.execute(
            """
            INSERT INTO artifact_transfers (
                id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                bytes_done, checksum_status, error, created_at, updated_at, completed_at
            ) VALUES (?, 'fixture-artifact-available', 'fixture-live', 'fixture-research',
                      ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                transfer_id,
                status,
                total,
                done,
                checksum,
                error,
                timestamp_ms,
                timestamp_ms,
                completed,
            ),
        )


def step_status_for(job_status: str, index: int, name: str) -> tuple[str, str | None]:
    """Return fixture step status and error for one job state."""
    if job_status == "completed":
        return "completed", None
    if job_status == "blocked":
        if index == 0:
            return "completed", None
        if index == 1:
            return "blocked", "validation requires operator review"
        return "queued", None
    if job_status == "failed":
        if name in {"run_backtest", "run_sweep"}:
            return "failed", "backtest command exited 1"
        if name == "write_report":
            return "queued", None
        return "completed", None
    if job_status == "cancelled":
        if index == 0:
            return "completed", None
        if index == 1:
            return "cancelled", "operator cancelled"
        return "queued", None
    if job_status == "running":
        return ("running", None) if index == 0 else ("queued", None)
    if job_status == "paused":
        if index == 0:
            return "completed", None
        if index == 1:
            return "paused", None
        return "queued", None
    return "queued", None


def insert_job(
    conn: sqlite3.Connection,
    job_id: str,
    job_type: str,
    status: str,
    timestamp_ms: int,
) -> None:
    """Insert one fixture job with deterministic steps and events."""
    artifact_id = None if job_type == "export" else "fixture-artifact-available"
    completed_at = timestamp_ms if status == "completed" else None
    cancelled_at = timestamp_ms if status == "cancelled" else None
    params = {
        "fixture": True,
        "state": status,
        "sweeps": ["EDGE_BPS"] if job_type == "sweep" else [],
    }
    conn.execute(
        """
        INSERT INTO research_jobs (
            id, job_type, artifact_id, status, priority, requested_by, params_json,
            created_at, updated_at, cancelled_at, completed_at
        ) VALUES (?, ?, ?, ?, 0, 'fixture-user-researcher', ?, ?, ?, ?, ?)
        """,
        (
            job_id,
            job_type,
            artifact_id,
            status,
            json_text(params),
            timestamp_ms,
            timestamp_ms,
            cancelled_at,
            completed_at,
        ),
    )
    for index, name in enumerate(STEP_TEMPLATES[job_type]):
        step_id = f"{job_id}-step-{index}"
        step_status, error = step_status_for(status, index, name)
        started_at = (
            timestamp_ms
            if step_status in {"running", "completed", "blocked", "failed"}
            else None
        )
        completed = timestamp_ms if step_status == "completed" else None
        conn.execute(
            """
            INSERT INTO research_job_steps (
                id, job_id, step_index, name, status, lease_owner, leased_until_ms,
                attempts, input_json, output_json, error, created_at, updated_at,
                started_at, completed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)
            """,
            (
                step_id,
                job_id,
                index,
                name,
                step_status,
                "fixture-worker" if step_status == "running" else None,
                timestamp_ms + 300_000 if step_status == "running" else None,
                1 if step_status != "queued" else 0,
                json_text({"fixture_step": name, "status": step_status})
                if step_status == "completed"
                else None,
                error,
                timestamp_ms,
                timestamp_ms,
                started_at,
                completed,
            ),
        )
    conn.execute(
        """
        INSERT INTO research_job_events (
            id, job_id, step_id, timestamp_ms, level, message, details_json
        ) VALUES (?, ?, NULL, ?, ?, ?, ?)
        """,
        (
            f"{job_id}-event-0",
            job_id,
            timestamp_ms,
            "warn" if status in {"blocked", "failed"} else "info",
            f"fixture job is {status}",
            json_text({"fixture": True, "status": status}),
        ),
    )


def report_payload(report_id: str, job_id: str, status: str) -> dict:
    """Return representative chart-ready fixture report JSON."""
    return {
        "schema_version": 1,
        "fixture": True,
        "report_id": report_id,
        "job_id": job_id,
        "status": status,
        "params": {
            "balance": 10000,
            "start": "2026-05-14T18:41:19Z",
            "end": "2026-05-17T07:57:06Z",
            "sets": {"EDGE_BPS": 2.5, "TAKE_PROFIT_BPS": 18},
        },
        "metrics": {
            "net_pnl": 284.25,
            "max_drawdown": -91.4,
            "win_rate": 0.58,
            "trade_count": 43,
        },
        "equity_curve": [
            {"timestamp_ms": 1779000000000, "equity": 10000.0},
            {"timestamp_ms": 1779001200000, "equity": 10074.5},
            {"timestamp_ms": 1779002400000, "equity": 10028.3},
            {"timestamp_ms": 1779003600000, "equity": 10112.9},
            {"timestamp_ms": 1779004800000, "equity": 10284.25},
        ],
        "sweep_points": [
            {"EDGE_BPS": 1.0, "net_pnl": 126.8, "max_drawdown": -74.2},
            {"EDGE_BPS": 2.5, "net_pnl": 284.25, "max_drawdown": -91.4},
            {"EDGE_BPS": 4.0, "net_pnl": 211.6, "max_drawdown": -143.7},
        ],
    }


def write_report_files(work_root: Path, report_id: str, job_id: str, status: str) -> tuple[str, str]:
    """Write fixture report files and return their paths."""
    report_root = work_root / "jobs" / job_id
    report_root.mkdir(parents=True, exist_ok=True)
    report_path = report_root / f"{report_id}.json"
    csv_path = report_root / f"{report_id}.csv"
    report_path.write_text(
        json.dumps(report_payload(report_id, job_id, status), indent=2),
        encoding="utf-8",
    )
    csv_path.write_text(
        "metric,value\n"
        "net_pnl,284.25\n"
        "max_drawdown,-91.4\n"
        "win_rate,0.58\n"
        "trade_count,43\n",
        encoding="utf-8",
    )
    return str(report_path), str(csv_path)


def insert_jobs_and_reports(
    conn: sqlite3.Connection, work_root: Path, timestamp_ms: int
) -> None:
    """Insert fixture jobs and representative reports."""
    jobs = [
        ("fixture-job-completed", "current_params", "completed"),
        ("fixture-job-blocked", "current_params", "blocked"),
        ("fixture-job-failed", "sweep", "failed"),
        ("fixture-job-cancelled", "export", "cancelled"),
        ("fixture-job-running", "current_params", "running"),
        ("fixture-job-paused", "current_params", "paused"),
    ]
    for job_id, job_type, status in jobs:
        insert_job(conn, job_id, job_type, status, timestamp_ms)
    reports = [
        ("fixture-report-available", "fixture-job-completed", "available", True),
        ("fixture-report-archived", "fixture-job-failed", "archived", True),
        ("fixture-report-missing-file", "fixture-job-blocked", "available", False),
    ]
    for report_id, job_id, status, write_files in reports:
        if write_files:
            report_path, csv_path = write_report_files(work_root, report_id, job_id, status)
        else:
            missing_root = work_root / "jobs" / job_id
            report_path = str(missing_root / "missing-report.json")
            csv_path = str(missing_root / "missing-report.csv")
        conn.execute(
            """
            INSERT INTO research_reports (
                id, job_id, artifact_id, title, status, summary_json, report_path,
                csv_path, created_at, updated_at
            ) VALUES (?, ?, 'fixture-artifact-available', ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                report_id,
                job_id,
                report_id.replace("-", " ").title(),
                status,
                json_text({"fixture": True, "job_id": job_id, "status": status}),
                report_path,
                csv_path,
                timestamp_ms,
                timestamp_ms,
            ),
        )


def seed(db_path: Path, work_root: Path, reset: bool) -> dict:
    """Seed the fixture dataset and return a summary."""
    db_path = db_path.expanduser().resolve()
    work_root = work_root.expanduser().resolve()
    db_path.parent.mkdir(parents=True, exist_ok=True)
    work_root.mkdir(parents=True, exist_ok=True)
    timestamp_ms = now_ms()
    conn = sqlite3.connect(db_path)
    try:
        conn.execute("PRAGMA foreign_keys = ON")
        conn.executescript(SCHEMA)
        if reset:
            reset_fixture_rows(conn, work_root)
        elif fixture_rows_exist(conn):
            raise RuntimeError(
                "fixture rows already exist; rerun with --reset to replace them"
            )
        ensure_fixture_user(conn, timestamp_ms)
        insert_machines(conn, timestamp_ms)
        insert_artifacts(conn, work_root, timestamp_ms)
        insert_transfers(conn, timestamp_ms)
        insert_jobs_and_reports(conn, work_root, timestamp_ms)
        conn.commit()
        counts = {
            table: conn.execute(
                f"SELECT COUNT(*) FROM {table} WHERE id LIKE 'fixture-%'"
            ).fetchone()[0]
            for table in [
                "research_machines",
                "run_artifacts",
                "artifact_transfers",
                "research_jobs",
                "research_job_steps",
                "research_job_events",
                "research_reports",
            ]
        }
        return {"db": str(db_path), "work_root": str(work_root), "counts": counts}
    finally:
        conn.close()


def main() -> None:
    """CLI entry point."""
    args = parse_args()
    try:
        summary = seed(Path(args.db), Path(args.work_root), args.reset)
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
