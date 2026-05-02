#!/usr/bin/env python3
"""Run the local live-money readiness gate and write an evidence bundle."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shlex
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
SECRET_MARKERS = (
    "AUTH",
    "CREDENTIAL",
    "KEY",
    "PASSPHRASE",
    "PASSWORD",
    "PRIVATE",
    "SECRET",
    "TOKEN",
)
ENV_ALLOWLIST = (
    "CI",
    "EXECUTION_MODE",
    "FEED_EVENT_STORAGE_PROFILE",
    "LIVE_",
    "NODE_ENV",
    "POLYMARKET_",
    "RUST_BACKTRACE",
    "RUST_LOG",
)

COMMANDS = [
    ("rust-live", ["cargo", "test", "-p", "buba-paint", "live"]),
    ("rust-live-control", ["cargo", "test", "-p", "buba-paint", "live_control"]),
    ("rust-live-fidelity", ["cargo", "test", "-p", "buba-paint", "live_fidelity"]),
    ("rust-replay", ["cargo", "test", "-p", "buba-paint", "replay"]),
    (
        "rust-live-system",
        ["cargo", "test", "-p", "buba-paint", "--test", "live_system_test"],
    ),
    ("sidecar-test", ["bash", "-lc", "cd polymarket-sidecar && npm test"]),
    ("sidecar-build", ["bash", "-lc", "cd polymarket-sidecar && npm run build"]),
    ("dashboard-test", ["bash", "-lc", "cd dashboard/client && npm test"]),
    ("dashboard-build", ["bash", "-lc", "cd dashboard/client && npm run build"]),
    ("lint", ["make", "lint"]),
    ("docs-audit", ["make", "docs-audit"]),
    ("comment-audit", ["make", "comment-audit"]),
    ("release-build", ["cargo", "build", "--release", "-p", "buba-paint"]),
    ("diff-check", ["git", "diff", "--check"]),
]


def utc_now() -> str:
    """Return an ISO-8601 UTC timestamp."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def default_output_dir() -> Path:
    """Return the default outside-repo evidence directory."""
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%SZ")
    return Path("/tmp") / f"buba-live-readiness-local-{stamp}"


def parse_args() -> argparse.Namespace:
    """Parse CLI arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        default=None,
        help="Evidence output directory. Defaults to /tmp/buba-live-readiness-local-<timestamp>.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Write the manifest and skipped command logs without executing gates.",
    )
    parser.add_argument(
        "--fail-fast",
        action="store_true",
        help="Stop after the first failed gate instead of collecting all failures.",
    )
    parser.add_argument(
        "--allow-repo-output",
        action="store_true",
        help="Allow output under the repository. Use only for debugging.",
    )
    return parser.parse_args()


def ensure_output_dir(path: Path, allow_repo_output: bool) -> Path:
    """Create and validate the output directory."""
    output_dir = path.expanduser().resolve()
    if not allow_repo_output:
        try:
            output_dir.relative_to(REPO_ROOT)
        except ValueError:
            pass
        else:
            raise SystemExit(
                f"refusing to write readiness evidence inside repo: {output_dir}"
            )
    output_dir.mkdir(parents=True, exist_ok=True)
    return output_dir


def run_text(command: list[str]) -> str:
    """Run a metadata command and return trimmed output."""
    try:
        result = subprocess.run(
            command,
            cwd=REPO_ROOT,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    except OSError as error:
        return f"unavailable: {error}"
    return result.stdout.strip()


def redact_env_value(key: str, value: str) -> str:
    """Return a redacted environment value when the key is sensitive."""
    upper_key = key.upper()
    if any(marker in upper_key for marker in SECRET_MARKERS):
        return "<redacted>"
    if len(value) > 160:
        return f"{value[:157]}..."
    return value


def selected_environment() -> dict[str, str]:
    """Return the selected redacted environment summary."""
    selected: dict[str, str] = {}
    for key, value in sorted(os.environ.items()):
        if any(key == prefix or key.startswith(prefix) for prefix in ENV_ALLOWLIST):
            selected[key] = redact_env_value(key, value)
    return selected


def host_info() -> dict[str, str]:
    """Return host metadata for the readiness manifest."""
    return {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "machine": platform.machine(),
    }


def command_string(command: list[str]) -> str:
    """Return a shell-readable command string."""
    return " ".join(shlex.quote(part) for part in command)


def write_text(path: Path, value: str) -> None:
    """Write one text file."""
    path.write_text(value, encoding="utf-8")


def run_gate(
    index: int,
    name: str,
    command: list[str],
    output_dir: Path,
    dry_run: bool,
) -> dict[str, Any]:
    """Run one readiness command and return its manifest entry."""
    started = utc_now()
    start_time = time.monotonic()
    log_file = f"{index:02d}-{name}.log"
    log_path = output_dir / log_file
    if dry_run:
        write_text(log_path, f"DRY RUN: {command_string(command)}\n")
        return {
            "name": name,
            "command": command,
            "status": "skipped_dry_run",
            "exit_code": 0,
            "log": log_file,
            "started_at_utc": started,
            "finished_at_utc": utc_now(),
            "duration_s": 0.0,
        }

    with log_path.open("w", encoding="utf-8") as log:
        log.write(f"$ {command_string(command)}\n")
        log.write(f"started_at_utc={started}\n\n")
        log.flush()
        result = subprocess.run(
            command,
            cwd=REPO_ROOT,
            check=False,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        finished = utc_now()
        duration = time.monotonic() - start_time
        log.write(f"\nfinished_at_utc={finished}\n")
        log.write(f"exit_code={result.returncode}\n")
    return {
        "name": name,
        "command": command,
        "status": "passed" if result.returncode == 0 else "failed",
        "exit_code": result.returncode,
        "log": log_file,
        "started_at_utc": started,
        "finished_at_utc": finished,
        "duration_s": round(duration, 3),
    }


def build_manifest(args: argparse.Namespace, output_dir: Path) -> dict[str, Any]:
    """Build the initial readiness manifest."""
    return {
        "kind": "buba_live_readiness_local",
        "generated_at_utc": utc_now(),
        "repo_root": str(REPO_ROOT),
        "output_dir": str(output_dir),
        "dry_run": bool(args.dry_run),
        "git_sha": run_text(["git", "rev-parse", "HEAD"]),
        "git_status_short": run_text(["git", "status", "--short"]),
        "host": host_info(),
        "environment": selected_environment(),
        "commands": [],
        "passed": False,
    }


def write_manifest(output_dir: Path, manifest: dict[str, Any]) -> None:
    """Write the readiness manifest."""
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    """Run the local readiness gate."""
    args = parse_args()
    output_dir = ensure_output_dir(
        Path(args.output_dir) if args.output_dir else default_output_dir(),
        args.allow_repo_output,
    )
    manifest = build_manifest(args, output_dir)
    write_manifest(output_dir, manifest)
    failures = 0
    for index, (name, command) in enumerate(COMMANDS, start=1):
        entry = run_gate(index, name, command, output_dir, args.dry_run)
        manifest["commands"].append(entry)
        write_manifest(output_dir, manifest)
        if entry["exit_code"] != 0:
            failures += 1
            if args.fail_fast:
                break
    manifest["finished_at_utc"] = utc_now()
    manifest["passed"] = failures == 0
    manifest["failure_count"] = failures
    write_manifest(output_dir, manifest)
    print(f"readiness_manifest={output_dir / 'manifest.json'}")
    print(f"readiness_passed={str(manifest['passed']).lower()}")
    return 0 if manifest["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
