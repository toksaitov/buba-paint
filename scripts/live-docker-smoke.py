#!/usr/bin/env python3
"""Run a short local no-Caddy Docker live_readonly smoke and collect evidence."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shlex
import shutil
import socket
import sqlite3
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
COMPOSE_FILES = [
    "docker-compose.yml",
    "docker-compose.live-readonly.yml",
    "docker-compose.smoke.yml",
]
FAILURE_PATTERNS = [
    "Bus error",
    "disk I/O error",
    "database disk image is malformed",
    "file is not a database",
]


def utc_now() -> str:
    """Return one UTC timestamp for evidence manifests."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def stamp() -> str:
    """Return one compact UTC timestamp for path names."""
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%SZ")


def default_output_dir() -> Path:
    """Return the default smoke evidence directory outside the repo."""
    return Path("/tmp") / f"buba-live-docker-smoke-{stamp()}"


def parse_args() -> argparse.Namespace:
    """Parse smoke-run arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration-seconds", type=int, default=600)
    parser.add_argument("--output-dir", default=None)
    parser.add_argument("--project", default=None)
    parser.add_argument("--sidecar-env", default=".secrets/buba-paint-live-sidecar.env")
    parser.add_argument("--dashboard-config", default="dashboard.toml")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--allow-repo-output", action="store_true")
    return parser.parse_args()


def ensure_output_dir(path: Path, allow_repo_output: bool) -> Path:
    """Create and validate an outside-repo evidence directory."""
    output_dir = path.expanduser().resolve()
    if not allow_repo_output:
        try:
            output_dir.relative_to(REPO_ROOT)
        except ValueError:
            pass
        else:
            raise SystemExit(f"refusing to write smoke evidence inside repo: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    return output_dir


def command_string(command: list[str]) -> str:
    """Return one shell-readable command string."""
    return " ".join(shlex.quote(part) for part in command)


def compose_command(args: list[str]) -> list[str]:
    """Return a docker compose command with the smoke compose files."""
    command = ["docker", "compose"]
    for compose_file in COMPOSE_FILES:
        command.extend(["-f", compose_file])
    command.extend(args)
    return command


def run_command(
    name: str,
    command: list[str],
    output_dir: Path,
    env: dict[str, str],
    check: bool = False,
) -> dict[str, Any]:
    """Run one command, save its log, and return manifest metadata."""
    started_at = utc_now()
    started = time.monotonic()
    log_path = output_dir / f"{name}.log"
    with log_path.open("w", encoding="utf-8") as log:
        log.write(f"$ {command_string(command)}\n")
        log.write(f"started_at_utc={started_at}\n\n")
        log.flush()
        result = subprocess.run(
            command,
            cwd=REPO_ROOT,
            env=env,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
        finished_at = utc_now()
        log.write(f"\nfinished_at_utc={finished_at}\n")
        log.write(f"exit_code={result.returncode}\n")
    if check and result.returncode != 0:
        raise RuntimeError(f"{name} failed with exit code {result.returncode}")
    return {
        "name": name,
        "command": command,
        "exit_code": result.returncode,
        "log": log_path.name,
        "started_at_utc": started_at,
        "finished_at_utc": finished_at,
        "duration_s": round(time.monotonic() - started, 3),
    }


def compose_container_ids(env: dict[str, str], include_stopped: bool = False) -> list[str]:
    """Return container IDs for the current compose project."""
    ps_args = ["ps", "-q"]
    if include_stopped:
        ps_args.append("-a")
    result = subprocess.run(
        compose_command(ps_args),
        cwd=REPO_ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def write_restart_summary(output_dir: Path, env: dict[str, str]) -> dict[str, Any] | None:
    """Write compact restart/health state for compose containers."""
    container_ids = compose_container_ids(env)
    if not container_ids:
        return None
    return run_command(
        "docker-restart-summary",
        [
            "docker",
            "inspect",
            "--format",
            "{{.Name}} restart_count={{.RestartCount}} status={{.State.Status}} health={{if .State.Health}}{{.State.Health.Status}}{{else}}n/a{{end}}",
            *container_ids,
        ],
        output_dir,
        env,
    )


def collect_compose_diagnostics(
    manifest: dict[str, Any], output_dir: Path, env: dict[str, str]
) -> None:
    """Collect compose state, service logs, inspect output, and copied runtime evidence."""
    manifest["commands"].append(
        run_command("compose-ps", compose_command(["ps"]), output_dir, env)
    )
    manifest["commands"].append(
        run_command(
            "compose-logs",
            compose_command(["logs", "--no-color", "--tail", "600"]),
            output_dir,
            env,
        )
    )
    restart_summary = write_restart_summary(output_dir, env)
    if restart_summary is not None:
        manifest["commands"].append(restart_summary)
    container_ids = compose_container_ids(env)
    if container_ids:
        manifest["commands"].append(
            run_command(
                "docker-inspect-summary",
                [
                    "docker",
                    "inspect",
                    "--format",
                    "{{.Name}} status={{.State.Status}} health={{if .State.Health}}{{.State.Health.Status}}{{else}}n/a{{end}} exit_code={{.State.ExitCode}} oom={{.State.OOMKilled}} started={{.State.StartedAt}} finished={{.State.FinishedAt}}",
                    *container_ids,
                ],
                output_dir,
                env,
            )
        )
    manifest["commands"].append(
        run_command("compose-stop", compose_command(["stop", "-t", "30"]), output_dir, env)
    )
    stopped_container_ids = compose_container_ids(env, include_stopped=True)
    if stopped_container_ids:
        manifest["commands"].append(
            run_command(
                "docker-inspect-post-stop-summary",
                [
                    "docker",
                    "inspect",
                    "--format",
                    "{{.Name}} status={{.State.Status}} exit_code={{.State.ExitCode}} oom={{.State.OOMKilled}} finished={{.State.FinishedAt}}",
                    *stopped_container_ids,
                ],
                output_dir,
                env,
            )
        )
    copied_runtime = output_dir / "runtime"
    if copied_runtime.exists():
        shutil.rmtree(copied_runtime)
    manifest["commands"].append(
        run_command(
            "compose-copy-runtime",
            compose_command(["cp", "paint:/runtime", str(output_dir)]),
            output_dir,
            env,
        )
    )


def read_text(path: Path) -> str:
    """Return text from one evidence file when available."""
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def sqlite_scalar(db_path: Path, sql: str) -> Any:
    """Run one scalar SQLite query against copied smoke evidence."""
    with sqlite3.connect(db_path) as conn:
        return conn.execute(sql).fetchone()[0]


def sqlite_rows(db_path: Path, sql: str) -> list[tuple[Any, ...]]:
    """Run one SQLite query and return all rows."""
    with sqlite3.connect(db_path) as conn:
        return conn.execute(sql).fetchall()


def ms_to_utc_iso(timestamp_ms: int) -> str:
    """Convert one millisecond Unix timestamp to a CLI-friendly UTC timestamp."""
    return datetime.fromtimestamp(timestamp_ms / 1000, timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )


def run_replay_validation(
    manifest: dict[str, Any], output_dir: Path, env: dict[str, str]
) -> None:
    """Run offline replay validation for copied smoke DB evidence when possible."""
    db_path = output_dir / "runtime" / "paint.db"
    if not db_path.exists():
        return
    try:
        start_ms, end_ms, rows = sqlite_rows(
            db_path,
            "SELECT MIN(received_at_ms), MAX(received_at_ms), COUNT(*) FROM feed_events",
        )[0]
    except (IndexError, sqlite3.Error) as error:
        manifest.setdefault("checks", {})["replay_validation_error"] = str(error)
        return
    if rows == 0 or start_ms is None or end_ms is None:
        manifest.setdefault("checks", {})["replay_validation_skipped"] = "no feed events"
        return
    command = [
        "cargo",
        "run",
        "-p",
        "buba-paint",
        "--",
        "validate-replay-data",
        "--data",
        str(db_path),
        "--start",
        ms_to_utc_iso(int(start_ms)),
        "--end",
        ms_to_utc_iso(int(end_ms) + 1_000),
    ]
    result = run_command("validate-replay-data", command, output_dir, env)
    manifest["commands"].append(result)
    checks = manifest.setdefault("checks", {})
    checks["replay_validation_exit_code"] = result["exit_code"]
    if result["exit_code"] != 0:
        checks["accepted"] = False
        checks["reason"] = "validate-replay-data failed"


def inspect_runtime(output_dir: Path) -> dict[str, Any]:
    """Inspect copied runtime logs and database for smoke acceptance."""
    runtime_dir = output_dir / "runtime"
    db_path = runtime_dir / "paint.db"
    checks: dict[str, Any] = {"runtime_dir": str(runtime_dir), "db_exists": db_path.exists()}
    failures: list[str] = []
    restart_text = read_text(output_dir / "docker-restart-summary.log")
    restart_failures: list[str] = []
    for line in restart_text.splitlines():
        if "restart_count=" not in line:
            continue
        if not line.startswith("/"):
            continue
        try:
            restart_count = int(line.split("restart_count=", 1)[1].split(" ", 1)[0])
        except (IndexError, ValueError):
            restart_failures.append(f"unparseable restart summary: {line}")
            continue
        if restart_count > 0:
            restart_failures.append(line)
    checks["restart_failures"] = restart_failures
    failures.extend(f"container restarted: {line}" for line in restart_failures)
    post_stop_text = read_text(output_dir / "docker-inspect-post-stop-summary.log")
    stop_failures: list[str] = []
    for line in post_stop_text.splitlines():
        if not line.startswith("/"):
            continue
        if "oom=true" in line or "exit_code=137" in line:
            stop_failures.append(line)
    checks["stop_failures"] = stop_failures
    failures.extend(f"container did not stop cleanly: {line}" for line in stop_failures)
    for log_name in ["paint.log", "agent.log", "dashboard.log", "sidecar.log"]:
        text = read_text(runtime_dir / log_name)
        for pattern in FAILURE_PATTERNS:
            if pattern in text:
                failures.append(f"{log_name}: {pattern}")
    checks["failure_patterns"] = failures
    if not db_path.exists():
        checks["accepted"] = False
        checks["reason"] = "paint.db was not copied from smoke runtime"
        return checks
    try:
        checks["quick_check"] = sqlite_scalar(db_path, "PRAGMA quick_check;")
        checks["feed_event_classes"] = sqlite_rows(
            db_path,
            "SELECT source, event_type, COUNT(*) FROM feed_events GROUP BY source, event_type ORDER BY source, event_type",
        )
        checks["live_order_intents"] = sqlite_scalar(
            db_path, "SELECT COUNT(*) FROM live_order_intents"
        )
        checks["live_orders"] = sqlite_scalar(db_path, "SELECT COUNT(*) FROM live_orders")
        checks["live_redemptions"] = sqlite_scalar(
            db_path, "SELECT COUNT(*) FROM live_redemptions"
        )
    except sqlite3.Error as error:
        checks["accepted"] = False
        checks["reason"] = f"sqlite inspection failed: {error}"
        return checks
    checks["accepted"] = (
        checks["quick_check"] == "ok"
        and not failures
        and checks["live_order_intents"] == 0
        and checks["live_orders"] == 0
        and checks["live_redemptions"] == 0
    )
    return checks


def host_info() -> dict[str, str]:
    """Return host metadata for the smoke manifest."""
    return {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "docker_desktop_note": "macOS bind-mounted SQLite WAL is not accepted for this smoke; docker-compose.smoke.yml uses a Docker-native volume.",
    }


def main() -> int:
    """Run the local Docker smoke workflow."""
    args = parse_args()
    output_dir = ensure_output_dir(
        Path(args.output_dir) if args.output_dir else default_output_dir(),
        args.allow_repo_output,
    )
    project = args.project or f"buba-smoke-{stamp().lower()}"
    env = os.environ.copy()
    env.update(
        {
            "COMPOSE_PROJECT_NAME": project,
            "BUBA_UID": str(os.getuid()),
            "BUBA_GID": str(os.getgid()),
            "BUBA_SIDECAR_ENV": args.sidecar_env,
            "BUBA_DASHBOARD_CONFIG": args.dashboard_config,
        }
    )
    manifest: dict[str, Any] = {
        "started_at_utc": utc_now(),
        "project": project,
        "duration_seconds": args.duration_seconds,
        "compose_files": COMPOSE_FILES,
        "output_dir": str(output_dir),
        "host": host_info(),
        "commands": [],
        "checks": {},
        "accepted": False,
    }
    if args.dry_run:
        manifest["accepted"] = True
        manifest["dry_run"] = True
        (output_dir / "manifest.json").write_text(
            json.dumps(manifest, indent=2), encoding="utf-8"
        )
        print(output_dir)
        return 0
    try:
        manifest["commands"].append(
            run_command("compose-config", compose_command(["config", "--quiet"]), output_dir, env)
        )
        compose_up = run_command(
            "compose-up", compose_command(["up", "-d", "--build"]), output_dir, env
        )
        manifest["commands"].append(compose_up)
        if compose_up["exit_code"] == 0:
            time.sleep(max(1, args.duration_seconds))
        else:
            manifest["checks"] = {
                "accepted": False,
                "reason": "docker compose up failed before smoke duration",
            }
        collect_compose_diagnostics(manifest, output_dir, env)
        manifest["checks"] = inspect_runtime(output_dir)
        run_replay_validation(manifest, output_dir, env)
        if compose_up["exit_code"] != 0:
            manifest["checks"]["accepted"] = False
            manifest["checks"]["reason"] = "docker compose up failed before smoke duration"
    finally:
        manifest["commands"].append(
            run_command("compose-down", compose_command(["down", "-v"]), output_dir, env)
        )
        manifest["finished_at_utc"] = utc_now()
        manifest["accepted"] = bool(manifest.get("checks", {}).get("accepted"))
        (output_dir / "manifest.json").write_text(
            json.dumps(manifest, indent=2, default=str), encoding="utf-8"
        )
    print(output_dir)
    return 0 if manifest["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
