#!/usr/bin/env python3
"""Operate research deployment backup, restore, rollback, and diagnostics."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_INVENTORY = REPO_ROOT / "ops" / "research-machines.toml"
DEPLOY_SCRIPT = REPO_ROOT / "scripts" / "deploy-machine.py"
SECRET_KEY_RE = re.compile(r"(PASSWORD|SECRET|TOKEN|PRIVATE|CREDENTIAL|AUTH|KEY)", re.IGNORECASE)
LOCK_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
BACKUP_ID_RE = re.compile(r"^[A-Za-z0-9_.-]+$")
REQUIRED_LOCK_IMAGES = ("dashboard", "research_worker")


def load_deploy_module() -> Any:
    """Load deploy-machine.py as an importable module."""
    scripts_dir = str(REPO_ROOT / "scripts")
    if scripts_dir not in sys.path:
        sys.path.insert(0, scripts_dir)
    spec = importlib.util.spec_from_file_location("deploy_machine", DEPLOY_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {DEPLOY_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


DEPLOY = load_deploy_module()


def parse_args() -> argparse.Namespace:
    """Parse CLI arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    subparsers = parser.add_subparsers(dest="command", required=True)

    status = add_machine_parser(subparsers, "status")
    status.set_defaults(func=cmd_status)

    backup = add_machine_parser(subparsers, "backup-db")
    backup.set_defaults(func=cmd_backup_db)

    restore = add_machine_parser(subparsers, "restore-db")
    restore.add_argument("--backup", required=True)
    restore.add_argument("--confirm", action="store_true")
    restore.set_defaults(func=cmd_restore_db)

    diagnostics = add_machine_parser(subparsers, "collect-diagnostics")
    diagnostics.set_defaults(func=cmd_collect_diagnostics)

    rollback = add_machine_parser(subparsers, "rollback")
    rollback.add_argument("--to-ref", required=True)
    rollback.add_argument("--confirm", action="store_true")
    rollback.set_defaults(func=cmd_rollback)

    live = add_machine_parser(subparsers, "live-safety")
    live.set_defaults(func=cmd_live_safety)

    return parser.parse_args()


def add_machine_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser], name: str) -> argparse.ArgumentParser:
    """Add shared machine arguments to a subparser."""
    parser = subparsers.add_parser(name)
    parser.add_argument("--machine", required=True)
    parser.add_argument("--dry-run", action="store_true")
    return parser


def load_inventory(path: Path) -> dict[str, Any]:
    """Load the deployment inventory."""
    return DEPLOY.load_inventory(path)


def plan_for(args: argparse.Namespace, *, allow_stale_image_lock: bool = False, image_lock_override: Path | None = None) -> dict[str, Any]:
    """Return the deploy-machine plan for command arguments."""
    inventory = load_inventory(args.inventory)
    return DEPLOY.machine_plan(
        inventory,
        args.machine,
        image_lock_override=image_lock_override,
        allow_stale_image_lock=allow_stale_image_lock,
    )


def require_research(plan: dict[str, Any]) -> None:
    """Require a research-machine plan."""
    if plan.get("machine") != "research" or plan.get("role") != "research":
        raise RuntimeError("this command is currently supported only for the research machine")


def require_live(plan: dict[str, Any]) -> None:
    """Require a live-machine plan."""
    if plan.get("role") != "live":
        raise RuntimeError("live-safety requires a live machine")


def print_json(payload: dict[str, Any]) -> int:
    """Print a JSON response and return success."""
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def dry_run_payload(command: str, plan: dict[str, Any], extra: dict[str, Any] | None = None) -> dict[str, Any]:
    """Return a dry-run response."""
    payload = {"command": command, "dry_run": True, "machine": plan}
    if extra:
        payload.update(extra)
    return payload


def quote(value: str) -> str:
    """Return a shell-quoted value."""
    return DEPLOY.quote(value)


def redact_secret_text(text: str) -> str:
    """Return text with secret-looking key/value lines redacted."""
    redacted_lines = []
    for line in text.splitlines():
        key = line.split("=", 1)[0].strip() if "=" in line else line
        if SECRET_KEY_RE.search(key):
            redacted_lines.append(f"{key}=<redacted>" if "=" in line else "<redacted>")
        else:
            redacted_lines.append(line)
    redacted = "\n".join(redacted_lines)
    redacted = re.sub(
        r'("(?:[^"]*(?:PASSWORD|SECRET|TOKEN|PRIVATE|CREDENTIAL|AUTH|KEY)[^"]*)"\s*:\s*)"[^"]*"',
        r'\1"<redacted>"',
        redacted,
        flags=re.IGNORECASE,
    )
    return redacted


def validate_backup_id(backup_id: str) -> None:
    """Validate a remote backup identifier."""
    if not BACKUP_ID_RE.fullmatch(backup_id):
        raise RuntimeError("backup id may contain only letters, numbers, dot, dash, and underscore")


def parse_backup_manifest_text(text: str) -> dict[str, Any]:
    """Parse and validate one backup manifest JSON payload."""
    payload = json.loads(text)
    backup_id = payload.get("backup_id")
    sha256 = payload.get("sha256")
    quick_check = payload.get("quick_check")
    db_bytes = payload.get("db_bytes")
    if not isinstance(backup_id, str):
        raise RuntimeError("backup manifest missing backup_id")
    validate_backup_id(backup_id)
    if not isinstance(sha256, str) or not re.fullmatch(r"[0-9a-f]{64}", sha256):
        raise RuntimeError("backup manifest sha256 is invalid")
    if quick_check != "ok":
        raise RuntimeError("backup manifest quick_check is not ok")
    if not isinstance(db_bytes, int) or db_bytes <= 0:
        raise RuntimeError("backup manifest db_bytes is invalid")
    return payload


def validate_lock_payload(lock: dict[str, Any]) -> None:
    """Validate a rollback lock payload."""
    images = lock.get("images")
    if not isinstance(images, dict):
        raise RuntimeError("image lock missing images object")
    for key in REQUIRED_LOCK_IMAGES:
        image = images.get(key)
        if not isinstance(image, dict):
            raise RuntimeError(f"image lock missing image metadata for {key}")
        ref = image.get("ref")
        digest = image.get("digest")
        if not isinstance(ref, str) or "@sha256:" not in ref:
            raise RuntimeError(f"image lock {key} is not digest pinned")
        if not isinstance(digest, str) or not LOCK_DIGEST_RE.fullmatch(digest):
            raise RuntimeError(f"image lock {key} digest is invalid")


def lock_summary(lock: dict[str, Any]) -> dict[str, Any]:
    """Return a compact lock summary."""
    validate_lock_payload(lock)
    return {
        "commit": lock.get("commit"),
        "tag": lock.get("tag"),
        "images": {
            key: lock["images"][key]["ref"]
            for key in REQUIRED_LOCK_IMAGES
        },
    }


def lock_from_ref(to_ref: str) -> tuple[dict[str, Any], str]:
    """Load a lock from a file path or git ref."""
    path = Path(to_ref)
    if path.exists():
        lock = json.loads(path.read_text(encoding="utf-8"))
        validate_lock_payload(lock)
        return lock, str(path)
    result = subprocess.run(
        ["git", "show", f"{to_ref}:ops/research-images.lock.json"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    lock = json.loads(result.stdout)
    validate_lock_payload(lock)
    return lock, to_ref


def write_temp_lock(lock: dict[str, Any]) -> Path:
    """Write one temporary image lock file."""
    handle = tempfile.NamedTemporaryFile("w", encoding="utf-8", prefix="buba-rollback-lock-", suffix=".json", delete=False)
    with handle:
        json.dump(lock, handle, indent=2, sort_keys=True)
        handle.write("\n")
    return Path(handle.name)


def remote_output(plan: dict[str, Any], script: str) -> str:
    """Run a remote script and return stdout."""
    return DEPLOY.remote_output(plan, script)


def cmd_status(args: argparse.Namespace) -> int:
    """Run the status command."""
    plan = plan_for(args)
    require_research(plan)
    if args.dry_run:
        return print_json(dry_run_payload("status", plan))
    return print_json(json.loads(remote_output(plan, remote_status_script(plan["remote_root"]))))


def cmd_backup_db(args: argparse.Namespace) -> int:
    """Run the DB backup command."""
    plan = plan_for(args)
    require_research(plan)
    if args.dry_run:
        return print_json(dry_run_payload("backup-db", plan, {"backup_root": f"{plan['remote_root']}/.docker/research/runtime/backups"}))
    return print_json(json.loads(remote_output(plan, remote_backup_script(plan["remote_root"]))))


def cmd_restore_db(args: argparse.Namespace) -> int:
    """Run the DB restore command."""
    plan = plan_for(args)
    require_research(plan)
    validate_backup_id(args.backup)
    if args.dry_run:
        return print_json(dry_run_payload("restore-db", plan, {"backup": args.backup, "requires_confirm": True}))
    if not args.confirm:
        raise RuntimeError("restore-db requires --confirm")
    return print_json(json.loads(remote_output(plan, remote_restore_script(plan["remote_root"], args.backup))))


def cmd_collect_diagnostics(args: argparse.Namespace) -> int:
    """Run the diagnostics collection command."""
    plan = plan_for(args)
    require_research(plan)
    if args.dry_run:
        return print_json(dry_run_payload("collect-diagnostics", plan, {"remote_tmp": "/tmp"}))
    return print_json(json.loads(remote_output(plan, remote_diagnostics_script(plan["remote_root"]))))


def cmd_rollback(args: argparse.Namespace) -> int:
    """Run the rollback command."""
    plan = plan_for(args)
    require_research(plan)
    lock, source = lock_from_ref(args.to_ref)
    if args.dry_run:
        return print_json(dry_run_payload("rollback", plan, {"lock_source": source, "rollback_lock": lock_summary(lock), "requires_confirm": True}))
    if not args.confirm:
        raise RuntimeError("rollback requires --confirm")
    return print_json(run_rollback(args, lock, source))


def cmd_live_safety(args: argparse.Namespace) -> int:
    """Run the live safety snapshot command."""
    plan = plan_for(args)
    require_live(plan)
    if args.dry_run:
        return print_json(dry_run_payload("live-safety", plan))
    script = "\n".join(
        [
            "set -euo pipefail",
            "python3 - <<'PY'",
            "import json, subprocess",
            "fmt = '{\"id\":\"{{.ID}}\",\"name\":\"{{.Names}}\",\"image\":\"{{.Image}}\",\"status\":\"{{.Status}}\",\"running_for\":\"{{.RunningFor}}\",\"state\":\"{{.State}}\"}'",
            "result = subprocess.run(['docker', 'ps', '--format', fmt], capture_output=True, text=True, check=True)",
            "containers = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]",
            "print(json.dumps({'containers': containers}, indent=2, sort_keys=True))",
            "PY",
        ]
    )
    return print_json(json.loads(remote_output(plan, script)))


def run_rollback(args: argparse.Namespace, rollback_lock: dict[str, Any], source: str) -> dict[str, Any]:
    """Deploy a rollback lock and then roll forward to the current lock."""
    rollback_path = write_temp_lock(rollback_lock)
    try:
        rollback_result = run_deploy_with_lock(args.inventory, rollback_path, allow_stale=True)
    except subprocess.CalledProcessError as error:
        roll_forward = run_current_deploy(args.inventory, check=False)
        return {
            "rollback": command_failure(error),
            "roll_forward_after_failure": roll_forward,
            "lock_source": source,
            "ok": False,
        }
    finally:
        rollback_path.unlink(missing_ok=True)

    roll_forward = run_current_deploy(args.inventory, check=True)
    return {
        "rollback": command_success(rollback_result),
        "roll_forward": roll_forward,
        "lock_source": source,
        "ok": True,
    }


def run_deploy_with_lock(inventory: Path, lock_path: Path, *, allow_stale: bool) -> subprocess.CompletedProcess[str]:
    """Run deploy-machine with an explicit lock path."""
    cmd = [
        sys.executable,
        str(DEPLOY_SCRIPT),
        "--inventory",
        str(inventory),
        "--machine",
        "research",
        "--image-lock-override",
        str(lock_path),
    ]
    if allow_stale:
        cmd.append("--allow-stale-image-lock")
    return subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True, check=True)


def run_current_deploy(inventory: Path, *, check: bool) -> dict[str, Any]:
    """Deploy the current tracked research lock."""
    result = subprocess.run(
        [
            sys.executable,
            str(DEPLOY_SCRIPT),
            "--inventory",
            str(inventory),
            "--machine",
            "research",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=check,
    )
    if result.returncode == 0:
        return command_success(result)
    return command_result(result)


def command_success(result: subprocess.CompletedProcess[str]) -> dict[str, Any]:
    """Return a successful command result."""
    return {**command_result(result), "ok": True}


def command_failure(error: subprocess.CalledProcessError) -> dict[str, Any]:
    """Return a failed command result."""
    return {
        "ok": False,
        "returncode": error.returncode,
        "stdout": tail_text(error.stdout or "", 20000),
        "stderr": tail_text(error.stderr or "", 20000),
    }


def command_result(result: subprocess.CompletedProcess[str]) -> dict[str, Any]:
    """Return a command result."""
    return {
        "returncode": result.returncode,
        "stdout": tail_text(result.stdout or "", 20000),
        "stderr": tail_text(result.stderr or "", 20000),
    }


def tail_text(text: str, limit: int) -> str:
    """Return a bounded text tail."""
    return text[-limit:]


def remote_status_script(root: str) -> str:
    """Return the remote status script."""
    return remote_python_script(root, STATUS_PY)


def remote_backup_script(root: str) -> str:
    """Return the remote DB backup script."""
    return remote_python_script(root, BACKUP_PY)


def remote_restore_script(root: str, backup_id: str) -> str:
    """Return the remote DB restore script."""
    return remote_python_script(root, RESTORE_PY, {"BACKUP_ID": backup_id})


def remote_diagnostics_script(root: str) -> str:
    """Return the remote diagnostics script."""
    return remote_python_script(root, DIAGNOSTICS_PY)


def remote_python_script(root: str, body: str, extra_env: dict[str, str] | None = None) -> str:
    """Wrap Python code in a remote shell script."""
    env_lines = [f"ROOT={quote(root)}"]
    for key, value in (extra_env or {}).items():
        env_lines.append(f"{key}={quote(value)}")
    return "\n".join(
        [
            "set -euo pipefail",
            f"cd {quote(root)}",
            f"{' '.join(env_lines)} python3 - <<'PY'",
            body.rstrip(),
            "PY",
        ]
    )


REMOTE_COMMON_PY = r'''
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import tarfile
import time
import urllib.request
from pathlib import Path

ROOT = Path(os.environ["ROOT"])
RUNTIME = ROOT / ".docker" / "research" / "runtime"
WORK = ROOT / ".docker" / "research" / "work"
DB = RUNTIME / "dashboard.db"
BACKUPS = RUNTIME / "backups"
COMPOSE = ["docker", "compose", "-f", "docker-compose.research.yml"]

def now_ms():
    return int(time.time() * 1000)

def stamp():
    return dt.datetime.now(dt.UTC).strftime("%Y%m%d-%H%M%S")

def run(args, check=False, env=None):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, env=env)
    if check and result.returncode != 0:
        raise RuntimeError(f"{args!r} failed: {result.stderr}")
    return {"returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr}

def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def quick_check(path):
    with sqlite3.connect(path) as conn:
        return conn.execute("PRAGMA quick_check;").fetchone()[0]

def db_counts(path):
    if not path.exists():
        return {"db_exists": False}
    counts = {"db_exists": True}
    with sqlite3.connect(path) as conn:
        for table in ("research_jobs", "research_reports", "run_artifacts", "artifact_transfers", "research_job_templates"):
            try:
                counts[table] = conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            except sqlite3.Error as error:
                counts[table] = {"error": str(error)}
    return counts

def env_map():
    values = {}
    env_path = ROOT / ".env"
    if not env_path.exists():
        return values
    for line in env_path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value.strip().strip("\"'")
    return values

def api_json(path):
    try:
        env = env_map()
        body = json.dumps({"username": env["ADMIN_USER"], "password": env["ADMIN_PASSWORD"]}).encode()
        req = urllib.request.Request("http://localhost:3002/api/auth/login", data=body, headers={"content-type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as response:
            token = json.loads(response.read())["token"]
        req = urllib.request.Request("http://localhost:3002" + path, headers={"authorization": f"Bearer {token}"})
        with urllib.request.urlopen(req, timeout=10) as response:
            return json.loads(response.read())
    except Exception as error:
        return {"error": str(error)}

def health():
    try:
        with urllib.request.urlopen("http://localhost:3002/health", timeout=10) as response:
            return json.loads(response.read())
    except Exception as error:
        return {"error": str(error)}

def compose_ps():
    result = run([*COMPOSE, "ps", "--format", "json"])
    rows = []
    for line in result["stdout"].splitlines():
        if line.strip():
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                rows.append({"raw": line})
    return {"returncode": result["returncode"], "rows": rows, "stderr": result["stderr"]}

def image_refs_from_ps(rows):
    return {row.get("Service") or row.get("Name"): row.get("Image") for row in rows if row.get("Image")}

def backup_ids():
    if not BACKUPS.exists():
        return []
    return sorted(path.name for path in BACKUPS.iterdir() if path.is_dir())

def backup_database(destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(DB) as source, sqlite3.connect(destination) as target:
        source.backup(target)

def group_counts(payload):
    if not isinstance(payload, dict):
        return {}
    counts = {}
    for key, value in payload.items():
        if isinstance(value, list):
            counts[key] = len(value)
        elif isinstance(value, dict):
            for nested_key in ("items", "rows", "jobs", "transfers", "artifacts", "reports"):
                nested_value = value.get(nested_key)
                if isinstance(nested_value, list):
                    counts[key] = len(nested_value)
                    break
    return counts

def summarize_queue(payload):
    if not isinstance(payload, dict) or "error" in payload:
        return payload
    summary = {}
    for key in ("queue_counts", "attention_counts", "retention_totals", "disabled_host_impact", "generated_at_ms"):
        if key in payload:
            summary[key] = payload[key]
    counts = group_counts(payload)
    if counts:
        summary["group_counts"] = counts
    return summary or {"keys": sorted(payload.keys())}

def summarize_retention(payload):
    if not isinstance(payload, dict) or "error" in payload:
        return payload
    summary = {}
    for key in ("totals", "estimated_bytes", "generated_at_ms"):
        if key in payload:
            summary[key] = payload[key]
    counts = group_counts(payload)
    if counts:
        summary["candidate_counts"] = counts
    return summary or {"keys": sorted(payload.keys())}

def summarize_templates(payload):
    if not isinstance(payload, (dict, list)) or (isinstance(payload, dict) and "error" in payload):
        return payload
    templates = payload if isinstance(payload, list) else payload.get("templates", [])
    if not isinstance(templates, list):
        return {"keys": sorted(payload.keys())}
    status_counts = {}
    type_counts = {}
    for template in templates:
        if not isinstance(template, dict):
            continue
        status = template.get("status") or "unknown"
        job_type = template.get("job_type") or "unknown"
        status_counts[status] = status_counts.get(status, 0) + 1
        type_counts[job_type] = type_counts.get(job_type, 0) + 1
    return {"count": len(templates), "status_counts": status_counts, "type_counts": type_counts}

def summarize_reports(payload):
    if not isinstance(payload, dict) or "error" in payload:
        return payload
    reports = payload.get("reports")
    if not isinstance(reports, list):
        return {"keys": sorted(payload.keys())}
    status_counts = {}
    for report in reports:
        if isinstance(report, dict):
            status = report.get("status") or "unknown"
            status_counts[status] = status_counts.get(status, 0) + 1
    return {"count": len(reports), "status_counts": status_counts}

def summarize_telemetry(payload):
    if not isinstance(payload, dict) or "error" in payload:
        return payload
    state = payload.get("telemetry")
    state_summary = None
    if isinstance(state, dict):
        state_summary = {
            key: state.get(key)
            for key in ("worker_id", "worker_version", "worker_status", "last_heartbeat_ms", "last_sample_ms", "last_error", "updated_at")
            if key in state
        }
        for key in ("host", "sampler", "activity"):
            value = state.get(key)
            if isinstance(value, dict):
                state_summary[key] = value
    machine = payload.get("machine")
    machine_summary = machine
    if isinstance(machine, dict):
        machine_summary = {
            key: machine.get(key)
            for key in ("id", "name", "role", "ssh_alias", "status", "updated_at")
            if key in machine
        }
    samples = payload.get("samples")
    return {
        "machine": machine_summary,
        "disabled": payload.get("disabled"),
        "stale": payload.get("stale"),
        "stale_after_ms": payload.get("stale_after_ms"),
        "dependency_counts": payload.get("dependency_counts"),
        "telemetry": state_summary,
        "sample_count": len(samples) if isinstance(samples, list) else None,
    }

def summarize_compose_ps(payload):
    if not isinstance(payload, dict):
        return payload
    rows = payload.get("rows")
    if not isinstance(rows, list):
        return payload
    services = []
    for row in rows:
        if not isinstance(row, dict):
            continue
        services.append({
            key: row.get(key)
            for key in ("Service", "Name", "ID", "Image", "State", "Status", "Health", "RunningFor", "CreatedAt")
            if key in row
        })
    return {"returncode": payload.get("returncode"), "services": services, "stderr": payload.get("stderr")}
'''

STATUS_PY = REMOTE_COMMON_PY + r'''
ps = compose_ps()
queue = api_json("/api/research/queue")
retention = api_json("/api/research/retention")
templates = api_json("/api/research/job-templates")
telemetry = api_json("/api/research/machines/research/telemetry")
payload = {
    "machine": "research",
    "remote_root": str(ROOT),
    "runtime": str(RUNTIME),
    "health": health(),
    "compose_ps": summarize_compose_ps(ps),
    "image_refs": image_refs_from_ps(ps["rows"]),
    "db": db_counts(DB),
    "backups": backup_ids(),
    "queue": summarize_queue(queue),
    "retention": summarize_retention(retention),
    "templates": summarize_templates(templates),
    "telemetry": summarize_telemetry(telemetry),
}
print(json.dumps(payload, indent=2, sort_keys=True))
'''

BACKUP_PY = REMOTE_COMMON_PY + r'''
backup_id = "dashboard-db-" + stamp()
backup_dir = BACKUPS / backup_id
backup_db = backup_dir / "dashboard.db"
backup_database(backup_db)
check = quick_check(backup_db)
if check != "ok":
    raise RuntimeError(f"backup quick_check failed: {check}")
ps = compose_ps()
manifest = {
    "backup_id": backup_id,
    "created_at_ms": now_ms(),
    "remote_root": str(ROOT),
    "source_db": str(DB),
    "backup_db": str(backup_db),
    "db_bytes": backup_db.stat().st_size,
    "sha256": sha256_file(backup_db),
    "quick_check": check,
    "compose_ps": ps,
    "image_refs": image_refs_from_ps(ps["rows"]),
    "counts": db_counts(backup_db),
}
(backup_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps({"backup": manifest}, indent=2, sort_keys=True))
'''

RESTORE_PY = REMOTE_COMMON_PY + r'''
backup_id = os.environ["BACKUP_ID"]
if not re.fullmatch(r"[A-Za-z0-9_.-]+", backup_id):
    raise RuntimeError("unsafe backup id")
backup_dir = BACKUPS / backup_id
manifest_path = backup_dir / "manifest.json"
backup_db = backup_dir / "dashboard.db"
if not manifest_path.exists() or not backup_db.exists():
    raise RuntimeError(f"backup not found: {backup_id}")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
if manifest.get("sha256") != sha256_file(backup_db):
    raise RuntimeError("backup sha256 mismatch")
check = quick_check(backup_db)
if check != "ok":
    raise RuntimeError(f"backup quick_check failed: {check}")
restore_env = os.environ.copy()
image_refs = manifest.get("image_refs") if isinstance(manifest, dict) else None
if isinstance(image_refs, dict):
    dashboard_image = image_refs.get("research-dashboard")
    worker_image = image_refs.get("research-worker")
    if isinstance(dashboard_image, str) and dashboard_image:
        restore_env["BUBA_DASHBOARD_IMAGE"] = dashboard_image
    if isinstance(worker_image, str) and worker_image:
        restore_env["BUBA_RESEARCH_WORKER_IMAGE"] = worker_image
pre_id = "pre-restore-" + stamp()
pre_dir = BACKUPS / pre_id
pre_db = pre_dir / "dashboard.db"
stopped = False
try:
    run([*COMPOSE, "stop", "research-worker", "research-dashboard"], check=True)
    stopped = True
    if DB.exists():
        backup_database(pre_db)
        pre_check = quick_check(pre_db)
        pre_manifest = {
            "backup_id": pre_id,
            "created_at_ms": now_ms(),
            "reason": "pre-restore safety backup",
            "source_db": str(DB),
            "backup_db": str(pre_db),
            "db_bytes": pre_db.stat().st_size,
            "sha256": sha256_file(pre_db),
            "quick_check": pre_check,
            "counts": db_counts(pre_db),
        }
        pre_dir.mkdir(parents=True, exist_ok=True)
        (pre_dir / "manifest.json").write_text(json.dumps(pre_manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    shutil.copy2(backup_db, DB)
    for sidecar in (DB.with_name(DB.name + "-wal"), DB.with_name(DB.name + "-shm")):
        sidecar.unlink(missing_ok=True)
    restored_check = quick_check(DB)
    if restored_check != "ok":
        raise RuntimeError(f"restored DB quick_check failed: {restored_check}")
    run([*COMPOSE, "up", "-d", "--no-build", "research-dashboard", "research-worker"], check=True, env=restore_env)
    stopped = False
    for _ in range(30):
        current_health = health()
        if current_health.get("ok") is True:
            break
        time.sleep(2)
    else:
        raise RuntimeError("dashboard did not become healthy after restore")
    queue = api_json("/api/research/queue")
    reports = api_json("/api/research/reports")
    templates = api_json("/api/research/job-templates")
    retention = api_json("/api/research/retention")
    telemetry = api_json("/api/research/machines/research/telemetry")
    verification = {
        "health": health(),
        "queue": summarize_queue(queue),
        "reports": summarize_reports(reports),
        "templates": summarize_templates(templates),
        "retention": summarize_retention(retention),
        "telemetry": summarize_telemetry(telemetry),
    }
    print(json.dumps({
        "restored_backup_id": backup_id,
        "pre_restore_backup_id": pre_id if pre_db.exists() else None,
        "quick_check": restored_check,
        "restored_image_env": {
            "BUBA_DASHBOARD_IMAGE": restore_env.get("BUBA_DASHBOARD_IMAGE"),
            "BUBA_RESEARCH_WORKER_IMAGE": restore_env.get("BUBA_RESEARCH_WORKER_IMAGE"),
        },
        "verification": verification,
    }, indent=2, sort_keys=True))
except Exception:
    if stopped:
        run([*COMPOSE, "up", "-d", "--no-build", "research-dashboard", "research-worker"], check=False)
    raise
'''

DIAGNOSTICS_PY = REMOTE_COMMON_PY + r'''
SECRET_RE = re.compile(r"(PASSWORD|SECRET|TOKEN|PRIVATE|CREDENTIAL|AUTH|KEY)", re.IGNORECASE)

def redact(text):
    lines = []
    for line in text.splitlines():
        key = line.split("=", 1)[0].strip() if "=" in line else line
        if SECRET_RE.search(key):
            lines.append(f"{key}=<redacted>" if "=" in line else "<redacted>")
        else:
            lines.append(line)
    redacted = "\n".join(lines)
    return re.sub(r'("(?:[^"]*(?:PASSWORD|SECRET|TOKEN|PRIVATE|CREDENTIAL|AUTH|KEY)[^"]*)"\s*:\s*)"[^"]*"', r'\1"<redacted>"', redacted, flags=re.IGNORECASE)

bundle_id = "buba-research-diagnostics-" + stamp()
out_dir = Path("/tmp") / bundle_id
out_dir.mkdir(parents=True, exist_ok=True)

def write(name, content):
    path = out_dir / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content if content.endswith("\n") else content + "\n", encoding="utf-8")

def write_json(name, value):
    write(name, json.dumps(value, indent=2, sort_keys=True))

write_json("manifest.json", {"bundle_id": bundle_id, "created_at_ms": now_ms(), "remote_root": str(ROOT)})
write_json("compose-ps.json", compose_ps())
compose_config = run([*COMPOSE, "config", "--no-interpolate"])
write("compose-config.txt", redact(compose_config["stdout"] + compose_config["stderr"]))
write_json("health.json", health())
env_path = ROOT / ".env"
write("env.redacted", redact(env_path.read_text(encoding="utf-8") if env_path.exists() else ""))
write("logs/dashboard.tail.log", (RUNTIME / "dashboard.log").read_text(encoding="utf-8", errors="replace")[-30000:] if (RUNTIME / "dashboard.log").exists() else "")
write("logs/research-worker.tail.log", (RUNTIME / "research-worker.log").read_text(encoding="utf-8", errors="replace")[-30000:] if (RUNTIME / "research-worker.log").exists() else "")
write_json("api/queue.json", api_json("/api/research/queue"))
write_json("api/retention.json", api_json("/api/research/retention"))
write_json("api/templates.json", api_json("/api/research/job-templates"))
write_json("api/jobs.json", api_json("/api/research/jobs"))
write_json("api/transfers.json", api_json("/api/research/transfers"))
write_json("api/artifacts.json", api_json("/api/research/artifacts"))
write_json("api/reports.json", api_json("/api/research/reports"))
write_json("api/telemetry.json", api_json("/api/research/machines/research/telemetry"))
write("disk.txt", run(["df", "-h", str(RUNTIME), str(WORK)])["stdout"] + run(["du", "-sh", str(RUNTIME), str(WORK)])["stdout"])
container_ids = run([*COMPOSE, "ps", "-q"])["stdout"].splitlines()
inspect = run(["docker", "inspect", *container_ids])["stdout"] if container_ids else "[]"
write("docker-inspect.redacted.json", redact(inspect))
tar_path = Path("/tmp") / f"{bundle_id}.tar.gz"
with tarfile.open(tar_path, "w:gz") as archive:
    archive.add(out_dir, arcname=bundle_id)
print(json.dumps({
    "bundle_id": bundle_id,
    "remote_path": str(tar_path),
    "bytes": tar_path.stat().st_size,
    "files": sorted(str(path.relative_to(out_dir)) for path in out_dir.rglob("*") if path.is_file()),
}, indent=2, sort_keys=True))
'''


def main() -> int:
    """Run the selected maintenance command."""
    args = parse_args()
    try:
        return args.func(args)
    except (OSError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
