#!/usr/bin/env python3
"""Run a standalone live-runtime profile and write evidence under /tmp."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import signal
import sqlite3
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


def utc_now() -> str:
    """Return one UTC timestamp for manifests."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def stamp() -> str:
    """Return one compact UTC timestamp for path names."""
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%SZ")


def default_output_dir() -> Path:
    """Return the default profiling evidence directory."""
    return Path("/tmp") / f"buba-live-runtime-profile-{stamp()}"


def parse_args() -> argparse.Namespace:
    """Parse profiler arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration-seconds", type=int, default=300)
    parser.add_argument("--sample-seconds", type=int, default=5)
    parser.add_argument("--output-dir", type=Path, default=default_output_dir())
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/buba-paint")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def ensure_output_dir(path: Path) -> Path:
    """Create and validate an outside-repo evidence directory."""
    output_dir = path.expanduser().resolve()
    try:
        output_dir.relative_to(ROOT)
    except ValueError:
        pass
    else:
        raise SystemExit(f"refusing to write profile evidence inside repo: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    return output_dir


def run_logged(command: list[str], output_dir: Path, name: str) -> dict[str, Any]:
    """Run one command and write its combined output."""
    log_path = output_dir / f"{name}.log"
    started = time.monotonic()
    with log_path.open("w", encoding="utf-8") as log:
        result = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
    return {
        "name": name,
        "command": command,
        "exit_code": result.returncode,
        "duration_s": round(time.monotonic() - started, 3),
        "log": log_path.name,
    }


def git_sha() -> str:
    """Return the current git SHA or unknown."""
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def profile_env() -> dict[str, str]:
    """Return a latency-only paper runtime environment."""
    env = os.environ.copy()
    env.update(
        {
            "EXECUTION_MODE": "paper",
            "FEED_EVENT_STORAGE_PROFILE": "replay_grade",
            "LATENCY_ARB_ENABLED": "true",
            "SPREAD_CAPTURE_ENABLED": "false",
            "CALM_PERSISTENCE_ENABLED": "false",
            "LIVE_FEED_BATCH_MAX_MESSAGES": env.get("LIVE_FEED_BATCH_MAX_MESSAGES", "64"),
        }
    )
    return env


def redact_env(env: dict[str, str]) -> dict[str, str]:
    """Return a compact redacted environment summary."""
    secret_markers = ("KEY", "SECRET", "TOKEN", "PASSWORD", "PRIVATE", "FUNDER")
    summary: dict[str, str] = {}
    for key, value in sorted(env.items()):
        if key.startswith(("EXECUTION_MODE", "FEED_EVENT", "LIVE_", "LATENCY_", "SPREAD_", "CALM_")):
            summary[key] = "<redacted>" if any(marker in key for marker in secret_markers) else value
    return summary


def collect_ps(pid: int, output_dir: Path, duration_s: int, sample_s: int) -> None:
    """Collect periodic process CPU snapshots."""
    deadline = time.monotonic() + duration_s
    log_path = output_dir / "process-samples.log"
    with log_path.open("w", encoding="utf-8") as log:
        while time.monotonic() < deadline:
            result = subprocess.run(
                ["ps", "-o", "pid,pcpu,pmem,rss,vsz,time,command", "-p", str(pid)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            log.write(result.stdout)
            log.write("\n")
            log.flush()
            time.sleep(max(1, sample_s))


def run_perf_if_available(pid: int, output_dir: Path, duration_s: int) -> subprocess.Popen[str] | None:
    """Start Linux perf sampling when the host supports it."""
    if platform.system() != "Linux" or shutil.which("perf") is None:
        return None
    data_path = output_dir / "perf.data"
    log_path = output_dir / "perf-record.log"
    log = log_path.open("w", encoding="utf-8")
    return subprocess.Popen(
        [
            "perf",
            "record",
            "-F",
            "199",
            "--call-graph",
            "dwarf",
            "-o",
            str(data_path),
            "-p",
            str(pid),
            "--",
            "sleep",
            str(duration_s),
        ],
        cwd=ROOT,
        text=True,
        stdout=log,
        stderr=subprocess.STDOUT,
    )


def sqlite_counts(db_path: Path) -> dict[str, Any]:
    """Return compact SQLite evidence counts."""
    if not db_path.exists():
        return {"exists": False}
    with sqlite3.connect(db_path) as conn:
        table_names = {
            row[0]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type = 'table'"
            ).fetchall()
        }

        def count_table(name: str) -> int | None:
            """Return one table count when the table exists in this DB shape."""
            if name not in table_names:
                return None
            return conn.execute(f"SELECT COUNT(*) FROM {name}").fetchone()[0]

        return {
            "exists": True,
            "quick_check": conn.execute("PRAGMA quick_check").fetchone()[0],
            "feed_events": count_table("feed_events"),
            "clob_replay_blocks": count_table("clob_replay_blocks"),
            "clob_replay_events": count_table("clob_replay_events"),
            "signals": count_table("signals"),
            "simulated_trades": count_table("simulated_trades"),
            "trade_results": count_table("trade_results"),
            "live_order_intents": count_table("live_order_intents"),
            "live_orders": count_table("live_orders"),
            "live_redemptions": count_table("live_redemptions"),
        }


def run_profile(args: argparse.Namespace, output_dir: Path) -> dict[str, Any]:
    """Run one standalone bot profile."""
    commands = []
    binary = args.binary.expanduser().resolve()
    if args.build:
        commands.append(
            run_logged(["cargo", "build", "--release", "-p", "buba-paint"], output_dir, "cargo-build")
        )
    if not binary.exists():
        raise RuntimeError(f"binary does not exist: {binary}")
    db_path = output_dir / "paint.db"
    log_path = output_dir / "paint.log"
    env = profile_env()
    started = utc_now()
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.Popen(
            [str(binary), "live", "--db-path", str(db_path), "--balance", "100"],
            cwd=ROOT,
            env=env,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        perf_process = run_perf_if_available(process.pid, output_dir, args.duration_seconds)
        collect_ps(process.pid, output_dir, args.duration_seconds, args.sample_seconds)
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)
        if perf_process is not None:
            perf_process.wait(timeout=max(10, args.duration_seconds + 30))
            if (output_dir / "perf.data").exists():
                commands.append(
                    run_logged(
                        ["perf", "report", "-i", str(output_dir / "perf.data"), "--stdio", "--no-children"],
                        output_dir,
                        "perf-report",
                    )
                )
    return {
        "started_at_utc": started,
        "finished_at_utc": utc_now(),
        "exit_code": process.returncode,
        "db": str(db_path),
        "log": str(log_path),
        "commands": commands,
        "sqlite": sqlite_counts(db_path),
        "environment": redact_env(env),
    }


def main() -> int:
    """Run the standalone live-runtime profiler."""
    args = parse_args()
    output_dir = ensure_output_dir(args.output_dir)
    manifest: dict[str, Any] = {
        "git_sha": git_sha(),
        "host": platform.node(),
        "platform": platform.platform(),
        "output_dir": str(output_dir),
        "duration_seconds": args.duration_seconds,
        "dry_run": args.dry_run,
    }
    if args.dry_run:
        manifest["binary"] = str(args.binary.expanduser())
        (output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        print(output_dir)
        return 0
    try:
        manifest["profile"] = run_profile(args, output_dir)
        manifest["status"] = "passed" if manifest["profile"]["exit_code"] == 0 else "failed"
    except Exception as exc:
        manifest["status"] = "failed"
        manifest["error"] = str(exc)
    (output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(output_dir)
    return 0 if manifest["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
