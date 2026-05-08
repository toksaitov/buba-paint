#!/usr/bin/env python3
"""Run local low-latency safety checks and write an evidence bundle."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def default_output_dir() -> Path:
    """Return the default evidence path outside the repository."""
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return Path("/tmp") / f"buba-paint-low-latency-{stamp}"


def command_plan() -> list[list[str]]:
    """Return the low-latency local command list."""
    return [
        ["python3", "scripts/audit-hot-path.py"],
        ["cargo", "test", "-p", "buba-paint", "live_feed_writer"],
        [
            "cargo",
            "test",
            "-p",
            "buba-paint",
            "live_bot_survives_concurrent_feed_disconnections",
        ],
        [
            "docker",
            "compose",
            "-f",
            "docker-compose.yml",
            "-f",
            "docker-compose.live-readonly.yml",
            "-f",
            "docker-compose.local.yml",
            "config",
        ],
    ]


def redact_env() -> dict[str, str]:
    """Return a redacted environment summary for evidence."""
    secret_markers = ("KEY", "SECRET", "TOKEN", "PASSWORD", "PRIVATE", "FUNDER")
    summary: dict[str, str] = {}
    for key, value in sorted(os.environ.items()):
        if key.startswith(("BUBA_", "RUST", "NODE", "DOCKER", "POLYMARKET")):
            summary[key] = "<redacted>" if any(marker in key for marker in secret_markers) else value
    return summary


def run_command(command: list[str], output_dir: Path, index: int) -> dict[str, object]:
    """Run one command and write its combined output."""
    log_path = output_dir / f"{index:02d}-{'-'.join(command[:3]).replace('/', '_')}.log"
    started = time.time()
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
    return {
        "command": command,
        "exit_code": process.returncode,
        "duration_s": round(time.time() - started, 3),
        "log": str(log_path),
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


def main() -> int:
    """Run the local low-latency gate."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=default_output_dir())
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    output_dir = args.output_dir.resolve()
    if ROOT in output_dir.parents or output_dir == ROOT:
        print("output directory must be outside the repository", file=sys.stderr)
        return 2
    output_dir.mkdir(parents=True, exist_ok=True)
    plan = command_plan()
    manifest: dict[str, object] = {
        "started_at_ms": int(time.time() * 1000),
        "git_sha": git_sha(),
        "host": platform.node(),
        "platform": platform.platform(),
        "output_dir": str(output_dir),
        "environment": redact_env(),
        "commands": [],
        "dry_run": args.dry_run,
    }
    if args.dry_run:
        manifest["commands"] = [{"command": command, "exit_code": None} for command in plan]
        (output_dir / "manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n",
            encoding="utf-8",
        )
        print(output_dir)
        return 0
    command_results = []
    failed = False
    for index, command in enumerate(plan, start=1):
        result = run_command(command, output_dir, index)
        command_results.append(result)
        failed = failed or result["exit_code"] != 0
    manifest["commands"] = command_results
    manifest["finished_at_ms"] = int(time.time() * 1000)
    manifest["status"] = "failed" if failed else "passed"
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n",
        encoding="utf-8",
    )
    print(output_dir)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
