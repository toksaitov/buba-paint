#!/usr/bin/env python3
"""Stage and run the no-order live_readonly host soak on buba-paint."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shlex
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_HOST = "buba-paint"
DEFAULT_REMOTE_ROOT = "/home/ubuntu/buba-paint-live"
DEFAULT_OUTPUT_DIR = REPO_ROOT / "data" / "experiments" / "replay-grade-readonly-soak-001"
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
SECRET_FIELD_RE = re.compile(
    r'"([A-Za-z0-9_]*(?:SIGNATURE|API_KEY|PASSPHRASE|PASSWORD|PRIVATE_KEY|SECRET|TOKEN|AUTHORIZATION|COOKIE)[A-Za-z0-9_]*)"\s*:\s*"[^"]*"',
    re.IGNORECASE,
)
SECRET_ASSIGNMENT_RE = re.compile(
    r"\b([A-Za-z0-9_]*(?:SIGNATURE|API_KEY|PASSPHRASE|PASSWORD|PRIVATE_KEY|SECRET|TOKEN|AUTHORIZATION|COOKIE)[A-Za-z0-9_]*)=([^\s,}]+)",
    re.IGNORECASE,
)


def utc_now() -> str:
    """Return an ISO-8601 UTC timestamp."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def stamp_now() -> str:
    """Return a filesystem-safe UTC stamp."""
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%SZ")


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default=DEFAULT_HOST)
    parser.add_argument("--remote-root", default=DEFAULT_REMOTE_ROOT)
    parser.add_argument("--duration-seconds", type=int, default=90 * 60)
    parser.add_argument("--poll-seconds", type=int, default=5 * 60)
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR))
    parser.add_argument("--release-stamp", default=None)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--skip-stop", action="store_true")
    return parser.parse_args()


def command_string(command: list[str]) -> str:
    """Return a shell-readable command string."""
    return " ".join(shlex.quote(part) for part in command)


def redact_text(value: str) -> str:
    """Redact obvious secret assignments from text."""
    value = SECRET_FIELD_RE.sub(lambda match: f'"{match.group(1)}":"<redacted>"', value)
    value = SECRET_ASSIGNMENT_RE.sub(lambda match: f"{match.group(1)}=<redacted>", value)
    lines = []
    for line in value.splitlines():
        stripped = line.lstrip()
        if "=" in stripped:
            key = stripped.split("=", 1)[0].strip().upper()
            if any(marker in key for marker in SECRET_MARKERS):
                prefix = line[: len(line) - len(stripped)]
                lines.append(f"{prefix}{stripped.split('=', 1)[0]}=<redacted>")
                continue
        lines.append(line)
    return "\n".join(lines)


def ensure_output_dir(path: Path) -> Path:
    """Create the local evidence directory."""
    output_dir = path.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    return output_dir


def write_json(path: Path, value: Any) -> None:
    """Write one JSON file."""
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_text(path: Path, value: str) -> None:
    """Write one text file."""
    path.write_text(value, encoding="utf-8")


def host_info() -> dict[str, str]:
    """Return local host metadata."""
    return {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "machine": platform.machine(),
    }


def run_text(command: list[str]) -> str:
    """Run a local metadata command and return trimmed output."""
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


class HostSoakRunner:
    """Stateful runner for the host readonly soak."""

    def __init__(self, args: argparse.Namespace) -> None:
        """Initialize paths, manifest, and remote names."""
        self.args = args
        self.output_dir = ensure_output_dir(Path(args.output_dir))
        self.release_stamp = args.release_stamp or stamp_now()
        self.release_name = f"phase9-readonly-soak-{self.release_stamp}"
        self.runtime_name = f"soak-001-{self.release_stamp}"
        self.remote_root = args.remote_root.rstrip("/")
        self.release_dir = f"{self.remote_root}/releases/{self.release_name}"
        self.runtime_dir = f"{self.remote_root}/runtime/{self.runtime_name}"
        self.manifest: dict[str, Any] = {
            "kind": "buba_live_readiness_host_soak",
            "generated_at_utc": utc_now(),
            "host": args.host,
            "dry_run": bool(args.dry_run),
            "duration_seconds": args.duration_seconds,
            "poll_seconds": args.poll_seconds,
            "release_name": self.release_name,
            "runtime_name": self.runtime_name,
            "release_dir": self.release_dir,
            "runtime_dir": self.runtime_dir,
            "local_host": host_info(),
            "git_sha": run_text(["git", "rev-parse", "HEAD"]),
            "git_status_short": run_text(["git", "status", "--short"]),
            "commands": [],
            "started_at_utc": None,
            "finished_at_utc": None,
            "passed": False,
            "failure": None,
        }
        write_json(self.output_dir / "manifest.json", self.manifest)

    def record(self, entry: dict[str, Any]) -> None:
        """Record one command result in the manifest."""
        self.manifest["commands"].append(entry)
        write_json(self.output_dir / "manifest.json", self.manifest)

    def run_local(self, name: str, command: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
        """Run one local command and log its output."""
        log_path = self.output_dir / f"{len(self.manifest['commands']) + 1:02d}-{name}.log"
        started = utc_now()
        if self.args.dry_run:
            write_text(log_path, f"DRY RUN: {command_string(command)}\n")
            entry = {
                "name": name,
                "command": command,
                "status": "skipped_dry_run",
                "exit_code": 0,
                "log": log_path.name,
                "started_at_utc": started,
                "finished_at_utc": utc_now(),
            }
            self.record(entry)
            return subprocess.CompletedProcess(command, 0, "", "")
        with log_path.open("w", encoding="utf-8") as log:
            log.write(f"$ {command_string(command)}\nstarted_at_utc={started}\n\n")
            log.flush()
            result = subprocess.run(
                command,
                cwd=REPO_ROOT,
                check=False,
                text=True,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            log.write(f"\nfinished_at_utc={utc_now()}\nexit_code={result.returncode}\n")
        entry = {
            "name": name,
            "command": command,
            "status": "passed" if result.returncode == 0 else "failed",
            "exit_code": result.returncode,
            "log": log_path.name,
            "started_at_utc": started,
            "finished_at_utc": utc_now(),
        }
        self.record(entry)
        if check and result.returncode != 0:
            raise RuntimeError(f"{name} failed; see {log_path}")
        return result

    def run_ssh(self, name: str, script: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Run one remote shell script through SSH."""
        command = [
            "ssh",
            self.args.host,
            "bash",
            "-lc",
            "source ~/.cargo/env 2>/dev/null || true; bash -s",
        ]
        log_path = self.output_dir / f"{len(self.manifest['commands']) + 1:02d}-{name}.log"
        started = utc_now()
        if self.args.dry_run:
            write_text(log_path, f"DRY RUN SSH {self.args.host}\n{script}\n")
            entry = {
                "name": name,
                "command": ["ssh", self.args.host, "<script redacted>"],
                "status": "skipped_dry_run",
                "exit_code": 0,
                "log": log_path.name,
                "started_at_utc": started,
                "finished_at_utc": utc_now(),
            }
            self.record(entry)
            return subprocess.CompletedProcess(command, 0, "", "")
        with log_path.open("w", encoding="utf-8") as log:
            log.write(f"$ ssh {shlex.quote(self.args.host)} < {name}.sh\n")
            log.write(f"started_at_utc={started}\n\n")
            log.flush()
            result = subprocess.run(
                command,
                cwd=REPO_ROOT,
                input=script,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
            log.write(redact_text(result.stdout))
            log.write(f"\nfinished_at_utc={utc_now()}\nexit_code={result.returncode}\n")
        entry = {
            "name": name,
            "command": ["ssh", self.args.host, "<script redacted>"],
            "status": "passed" if result.returncode == 0 else "failed",
            "exit_code": result.returncode,
            "log": log_path.name,
            "started_at_utc": started,
            "finished_at_utc": utc_now(),
        }
        self.record(entry)
        if check and result.returncode != 0:
            raise RuntimeError(f"{name} failed; see {log_path}")
        return result

    def capture_ssh(self, name: str, script: str, output_file: str, check: bool = True) -> None:
        """Run a remote command and save stdout to an evidence file."""
        command = [
            "ssh",
            self.args.host,
            "bash",
            "-lc",
            "source ~/.cargo/env 2>/dev/null || true; bash -s",
        ]
        log_path = self.output_dir / f"{len(self.manifest['commands']) + 1:02d}-{name}.log"
        output_path = self.output_dir / output_file
        started = utc_now()
        if self.args.dry_run:
            write_text(log_path, f"DRY RUN SSH CAPTURE {self.args.host}\n{script}\n")
            write_text(output_path, "")
            entry = {
                "name": name,
                "command": ["ssh", self.args.host, "<capture script redacted>"],
                "status": "skipped_dry_run",
                "exit_code": 0,
                "log": log_path.name,
                "output": output_file,
                "started_at_utc": started,
                "finished_at_utc": utc_now(),
            }
            self.record(entry)
            return
        result = subprocess.run(
            command,
            cwd=REPO_ROOT,
            input=script,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        write_text(output_path, redact_text(result.stdout))
        write_text(
            log_path,
            "\n".join(
                [
                    f"$ ssh {shlex.quote(self.args.host)} < {name}.sh",
                    f"started_at_utc={started}",
                    f"stderr={redact_text(result.stderr)}",
                    f"finished_at_utc={utc_now()}",
                    f"exit_code={result.returncode}",
                    "",
                ]
            ),
        )
        entry = {
            "name": name,
            "command": ["ssh", self.args.host, "<capture script redacted>"],
            "status": "passed" if result.returncode == 0 else "failed",
            "exit_code": result.returncode,
            "log": log_path.name,
            "output": output_file,
            "started_at_utc": started,
            "finished_at_utc": utc_now(),
        }
        self.record(entry)
        if check and result.returncode != 0:
            raise RuntimeError(f"{name} failed; see {log_path}")

    def ensure_local_artifacts(self) -> None:
        """Verify locally built frontend and sidecar artifacts exist."""
        missing = []
        for path in (
            REPO_ROOT / "dashboard" / "client" / "dist" / "index.html",
            REPO_ROOT / "polymarket-sidecar" / "dist" / "index.js",
        ):
            if not path.exists():
                missing.append(str(path.relative_to(REPO_ROOT)))
        if missing:
            raise RuntimeError(
                "missing local build artifacts from the local readiness gate: "
                + ", ".join(missing)
            )

    def create_remote_dirs(self) -> None:
        """Create remote release, runtime, config, and logs directories."""
        self.run_ssh(
            "remote-bootstrap-dirs",
            f"""
set -euo pipefail
ROOT={shlex.quote(self.remote_root)}
RELEASE={shlex.quote(self.release_dir)}
RUNTIME={shlex.quote(self.runtime_dir)}
mkdir -p "$ROOT/releases" "$ROOT/runtime" "$ROOT/config" "$ROOT/logs" "$RELEASE" "$RUNTIME"
chmod 700 "$ROOT/config"
echo "release=$RELEASE"
echo "runtime=$RUNTIME"
df -h "$ROOT" "$HOME" || true
""",
        )

    def rsync_release(self) -> None:
        """Stage the local tree to a fresh remote release directory."""
        command = [
            "rsync",
            "-az",
            "--delete",
            "--exclude=.git/",
            "--exclude=target/",
            "--exclude=data/",
            "--exclude=runs/",
            "--exclude=node_modules/",
            "--exclude=dashboard/client/node_modules/",
            "--exclude=dashboard/client/coverage/",
            "--exclude=dashboard/client/test-results/",
            "--exclude=dashboard/client/playwright-report/",
            "--exclude=polymarket-sidecar/node_modules/",
            "--exclude=polymarket-sidecar/coverage/",
            f"{REPO_ROOT}/",
            f"{self.args.host}:{self.release_dir}/",
        ]
        self.run_local("rsync-release", command)

    def configure_remote_release(self) -> None:
        """Build remote artifacts, install units, and write redacted-safe env files."""
        setup_script = f"""
set -euo pipefail
ROOT={shlex.quote(self.remote_root)}
RELEASE={shlex.quote(self.release_dir)}
RUNTIME={shlex.quote(self.runtime_dir)}
OLD_RUN="$ROOT/runtime/run-013"
CONFIG="$ROOT/config"
LOGS="$ROOT/logs"
cd "$RELEASE"
source "$HOME/.cargo/env" 2>/dev/null || true

if [ "$(loginctl show-user "$USER" -p Linger --value 2>/dev/null || echo no)" != "yes" ]; then
  sudo -n loginctl enable-linger "$USER"
fi

mkdir -p "$HOME/.config/systemd/user" "$CONFIG" "$LOGS" "$RUNTIME"
cp ops/systemd/*.service "$HOME/.config/systemd/user/"
systemctl --user daemon-reload

if [ ! -f "$CONFIG/sidecar.env" ]; then
  if [ ! -f "$OLD_RUN/sidecar.env" ]; then
    echo "no stable sidecar.env at $CONFIG/sidecar.env and no fallback at $OLD_RUN/sidecar.env" >&2
    exit 20
  fi
fi

python3 - <<'PY'
import json
import os
import secrets
from pathlib import Path

root = Path(os.environ["HOME"]) / "buba-paint-live"
runtime = Path({json.dumps(self.runtime_dir.replace("~", "$HOME"))}.replace("$HOME", os.environ["HOME"]))
old_run = root / "runtime" / "run-013"
config = root / "config"

def read_env(path: Path) -> dict[str, str]:
    values: dict[str, str] = {{}}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values

def write_env(path: Path, values: dict[str, str]) -> None:
    path.write_text("\\n".join(f"{{key}}={{value}}" for key, value in values.items()) + "\\n", encoding="utf-8")
    path.chmod(0o600)

stable_sidecar_env = config / "sidecar.env"
sidecar = read_env(stable_sidecar_env if stable_sidecar_env.exists() else old_run / "sidecar.env")
paint = read_env(old_run / "paint.env") if (old_run / "paint.env").is_file() else {{}}
agent_old = read_env(old_run / "agent.env") if (old_run / "agent.env").is_file() else {{}}
dashboard_old = read_env(old_run / "dashboard.env") if (old_run / "dashboard.env").is_file() else {{}}

for key in ("POLYMARKET_PRIVATE_KEY", "POLYMARKET_PROXY_WALLET"):
    if not sidecar.get(key):
        raise SystemExit(f"missing required sidecar credential {{key}}")

sidecar["BUBA_SIDECAR_LOG_PATH"] = str(runtime / "sidecar.log")
write_env(config / "sidecar.env", sidecar)

secret_prefixes = (
    "POLYMARKET_PRIVATE_KEY",
    "POLYMARKET_PROXY_WALLET",
    "POLYMARKET_FUNDER",
    "POLYMARKET_API_KEY",
    "POLYMARKET_API_SECRET",
    "POLYMARKET_API_PASSPHRASE",
    "POLYMARKET_BUILDER_",
    "POLYMARKET_RELAYER_API_KEY",
    "POLYMARKET_RELAYER_API_KEY_ADDRESS",
)
bot = {{
    key: value
    for key, value in paint.items()
    if not any(key == prefix or key.startswith(prefix) for prefix in secret_prefixes)
}}
bot.update({{
    "BUBA_PAINT_DB_PATH": str(runtime / "paint.db"),
    "BUBA_PAINT_LOG_PATH": str(runtime / "paint.log"),
    "BUBA_PAINT_BALANCE": "100",
    "DB_PATH": str(runtime / "paint.db"),
    "EXECUTION_MODE": "live_readonly",
    "FEED_EVENT_STORAGE_PROFILE": "replay_grade",
    "LATENCY_ARB_ENABLED": "true",
    "SPREAD_CAPTURE_ENABLED": "false",
    "CALM_PERSISTENCE_ENABLED": "false",
    "LIVE_SIDECAR_URL": "http://127.0.0.1:3210",
}})
write_env(config / "bot.env", bot)

agent_secret = secrets.token_hex(32)
agent = dict(agent_old)
agent.update({{
    "AGENT_SECRET": agent_secret,
    "BUBA_PAINT_DB_PATH": str(runtime / "paint.db"),
    "BUBA_PAINT_LOG_PATH": str(runtime / "paint.log"),
    "BUBA_AGENT_LOG_PATH": str(runtime / "agent.log"),
    "BUBA_AGENT_PORT": "9090",
}})
write_env(config / "agent.env", agent)

jwt_secret = secrets.token_hex(32)
admin_user = dashboard_old.get("ADMIN_USER") or "admin"
admin_password = dashboard_old.get("ADMIN_PASSWORD") or secrets.token_urlsafe(32)
admin_password_path = runtime / "dashboard-admin-password.txt"
admin_password_path.write_text(admin_password + "\\n", encoding="utf-8")
admin_password_path.chmod(0o600)
dashboard = dict(dashboard_old)
dashboard.update({{
    "JWT_SECRET": jwt_secret,
    "DASHBOARD_DB_PATH": str(runtime / "dashboard.db"),
    "BUBA_DASHBOARD_CONFIG": str(config / "dashboard.toml"),
    "BUBA_DASHBOARD_LOG_PATH": str(runtime / "dashboard.log"),
    "ADMIN_USER": admin_user,
    "ADMIN_PASSWORD": admin_password,
}})
write_env(config / "dashboard.env", dashboard)

dashboard_toml = "\\n".join([
    "[server]",
    "port = 3000",
    "jwt_secret = " + json.dumps(jwt_secret),
    "",
    "[[agents]]",
    'id = "paint-soak"',
    'name = "BTC Paint Readonly Soak"',
    'url = "http://127.0.0.1:9090"',
    "secret = " + json.dumps(agent_secret),
    "",
])
(config / "dashboard.toml").write_text(dashboard_toml, encoding="utf-8")
(config / "dashboard.toml").chmod(0o600)

report = {{
    "sidecar_env_keys": sorted(sidecar),
    "bot_env_keys": sorted(bot),
    "agent_env_keys": sorted(agent),
    "dashboard_env_keys": sorted(dashboard),
    "dashboard_login_user": admin_user,
    "dashboard_login_password_written_to": str(admin_password_path),
    "builder_relayer_credentials_present": all(sidecar.get(key) for key in (
        "POLYMARKET_BUILDER_API_KEY",
        "POLYMARKET_BUILDER_SECRET",
        "POLYMARKET_BUILDER_PASSPHRASE",
    )),
    "redemption_readiness": "available" if all(sidecar.get(key) for key in (
        "POLYMARKET_BUILDER_API_KEY",
        "POLYMARKET_BUILDER_SECRET",
        "POLYMARKET_BUILDER_PASSPHRASE",
    )) else "unavailable_missing_builder_credentials",
}}
(runtime / "env-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\\n", encoding="utf-8")
PY

cd "$RELEASE/polymarket-sidecar"
npm ci --omit=dev --ignore-scripts
cd "$RELEASE"
cargo build --release -p buba-paint -p buba-agent -p buba-dashboard

set -a
. "$CONFIG/bot.env"
set +a
"$RELEASE/target/release/buba-paint" init-db --db-path "$BUBA_PAINT_DB_PATH" --balance "$BUBA_PAINT_BALANCE"
""".replace("{json.dumps(self.runtime_dir.replace(\"~\", \"$HOME\"))}", json.dumps(self.runtime_dir.replace("~", "$HOME")))
        self.run_ssh("remote-configure-build", setup_script)

    def start_services(self) -> None:
        """Start the sidecar first so preflight can fail before bot startup."""
        self.run_ssh(
            "remote-start-sidecar",
            f"""
set -euo pipefail
ROOT={shlex.quote(self.remote_root)}
RELEASE={shlex.quote(self.release_dir)}
RUNTIME={shlex.quote(self.runtime_dir)}
cd "$RELEASE"
systemctl --user stop buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
systemctl --user reset-failed buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
ln -sfn "$RELEASE" "$ROOT/current"
systemctl --user start buba-polymarket-sidecar.service
for i in $(seq 1 60); do
  if curl -fsS http://127.0.0.1:3210/health >/dev/null; then break; fi
  sleep 1
done
curl -fsS http://127.0.0.1:3210/health >/dev/null
systemctl --user --no-pager status buba-polymarket-sidecar.service | sed -n '1,80p'
""",
        )

    def start_monitor_services(self) -> None:
        """Start the readonly bot, agent, and dashboard after preflight passes."""
        self.run_ssh(
            "remote-start-monitor-services",
            """
set -euo pipefail
systemctl --user start buba-paint-bot.service
sleep 5
systemctl --user start buba-agent.service
for i in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:9090/health >/dev/null; then break; fi
  sleep 1
done
curl -fsS http://127.0.0.1:9090/health >/dev/null
systemctl --user start buba-dashboard.service
for i in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:3000/health >/dev/null; then break; fi
  sleep 1
done
curl -fsS http://127.0.0.1:3000/health >/dev/null
systemctl --user --no-pager status buba-paint-bot.service buba-agent.service buba-dashboard.service | sed -n '1,160p'
""",
        )

    def stop_services(self) -> None:
        """Stop host services used by the soak."""
        self.run_ssh(
            "remote-stop-services",
            """
set -euo pipefail
systemctl --user stop buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
systemctl --user --no-pager status buba-polymarket-sidecar.service buba-paint-bot.service buba-agent.service buba-dashboard.service 2>/dev/null | sed -n '1,160p' || true
""",
            check=False,
        )

    def capture_standard_evidence(self, prefix: str) -> None:
        """Capture health, process, log, and DB evidence."""
        remote_prefix = shlex.quote(prefix)
        self.capture_ssh(
            f"{prefix}-health",
            """
set -euo pipefail
echo '{"sidecar":'
curl -fsS http://127.0.0.1:3210/health || true
echo ',"agent":'
curl -fsS http://127.0.0.1:9090/health || true
echo ',"dashboard":'
curl -fsS http://127.0.0.1:3000/health || true
echo '}'
""",
            f"{prefix}-health.json",
            check=False,
        )
        self.capture_ssh(
            f"{prefix}-processes",
            """
set -euo pipefail
ps -eo pid=,etime=,args= | awk '/buba-paint live|buba-agent|buba-dashboard|polymarket-sidecar|node dist\\/index.js/ && !/awk/ {print}'
systemctl --user --no-pager --plain list-units 'buba*' || true
""",
            f"{prefix}-processes.txt",
            check=False,
        )
        self.capture_ssh(
            f"{prefix}-logs",
            f"""
set -euo pipefail
RUNTIME={shlex.quote(self.runtime_dir)}
for name in sidecar paint agent dashboard; do
  path="$RUNTIME/$name.log"
  echo "===== $name.log ====="
  if [ -f "$path" ]; then tail -n 120 "$path"; else echo "missing $path"; fi
done
""",
            f"{prefix}-log-tail.txt",
            check=False,
        )
        self.capture_ssh(
            f"{prefix}-db",
            f"""
set -euo pipefail
RUNTIME={shlex.quote(self.runtime_dir)}
DB="$RUNTIME/paint.db"
if [ -f "$DB" ]; then
  echo "quick_check=$(sqlite3 "$DB" 'PRAGMA quick_check;')"
  echo "run_metadata"
  sqlite3 "$DB" "SELECT key || '=' || value FROM run_metadata ORDER BY key;" 2>/dev/null || true
  echo "live_sessions"
  sqlite3 "$DB" "SELECT execution_mode || ':' || status || ':' || COUNT(*) FROM live_sessions GROUP BY execution_mode, status;" 2>/dev/null || true
  echo "order_counts"
  sqlite3 "$DB" "SELECT 'live_order_intents=' || COUNT(*) FROM live_order_intents UNION ALL SELECT 'live_orders=' || COUNT(*) FROM live_orders UNION ALL SELECT 'live_fills=' || COUNT(*) FROM live_fills UNION ALL SELECT 'live_redemptions=' || COUNT(*) FROM live_redemptions;" 2>/dev/null || true
else
  echo "missing_db=$DB"
fi
""",
            f"{prefix}-db.txt",
            check=False,
        )

    def run_preflight(self) -> None:
        """Capture live preflight output."""
        self.capture_ssh(
            "live-preflight",
            f"""
set -euo pipefail
RELEASE={shlex.quote(self.release_dir)}
CONFIG={shlex.quote(self.remote_root)}/config
cd "$RELEASE"
set -a
. "$CONFIG/bot.env"
set +a
PREFLIGHT_JSON=$(mktemp)
"$RELEASE/target/release/buba-paint" live-preflight | tee "$PREFLIGHT_JSON"
python3 - "$PREFLIGHT_JSON" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))
if payload.get("ok") is not True:
    errors = payload.get("errors") or []
    raise SystemExit("live preflight failed: " + "; ".join(str(error) for error in errors))
PY
""",
            "live-preflight.json",
        )

    def capture_dashboard_summary(self) -> None:
        """Capture dashboard proxied trading summary without storing auth tokens."""
        self.capture_ssh(
            "dashboard-summary",
            f"""
set -euo pipefail
CONFIG={shlex.quote(self.remote_root)}/config
python3 - <<'PY'
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

def read_env(path: Path) -> dict[str, str]:
    values = {{}}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if "=" in raw and not raw.lstrip().startswith("#"):
            key, value = raw.strip().split("=", 1)
            values[key] = value
    return values

env = read_env(Path(os.environ["HOME"]) / "buba-paint-live" / "config" / "dashboard.env")
login_body = json.dumps({{"username": env["ADMIN_USER"], "password": env["ADMIN_PASSWORD"]}}).encode()
request = urllib.request.Request(
    "http://127.0.0.1:3000/api/auth/login",
    data=login_body,
    headers={{"Content-Type": "application/json"}},
)
with urllib.request.urlopen(request, timeout=10) as response:
    token = json.loads(response.read().decode())["token"]
summary_request = urllib.request.Request(
    "http://127.0.0.1:3000/api/bots/paint-soak/trading/summary",
    headers={{"Authorization": f"Bearer {{token}}"}},
)
with urllib.request.urlopen(summary_request, timeout=10) as response:
    print(json.dumps(json.loads(response.read().decode()), indent=2, sort_keys=True))
PY
""",
            "dashboard-trading-summary.json",
            check=False,
        )

    def run_replay_quality(self) -> None:
        """Run validate-replay-data against the soak interval."""
        started = self.manifest.get("started_at_utc")
        finished = self.manifest.get("finished_at_utc") or utc_now()
        if not started:
            raise RuntimeError("soak start time missing")
        self.capture_ssh(
            "validate-replay-data",
            f"""
set -euo pipefail
RELEASE={shlex.quote(self.release_dir)}
DB={shlex.quote(self.runtime_dir)}/paint.db
cd "$RELEASE"
"$RELEASE/target/release/buba-paint" validate-replay-data --data "$DB" --start {shlex.quote(str(started))} --end {shlex.quote(str(finished))}
""",
            "replay-quality.txt",
            check=True,
        )

    def run_acceptance_check(self) -> None:
        """Fail the phase when the host soak violates no-order acceptance rules."""
        self.run_ssh(
            "remote-acceptance-check",
            f"""
set -euo pipefail
RELEASE={shlex.quote(self.release_dir)}
RUNTIME={shlex.quote(self.runtime_dir)}
CONFIG={shlex.quote(self.remote_root)}/config
DB="$RUNTIME/paint.db"

python3 - <<'PY'
import json
import urllib.request

with urllib.request.urlopen("http://127.0.0.1:3210/health", timeout=10) as response:
    health = json.loads(response.read().decode())
if health.get("ready") is not True or health.get("readiness_status") != "ready":
    raise SystemExit(f"sidecar health is not ready: {{health}}")

with urllib.request.urlopen("http://127.0.0.1:9090/health", timeout=10) as response:
    agent = json.loads(response.read().decode())
if agent.get("ok") is not True:
    raise SystemExit(f"agent health is not ok: {{agent}}")

with urllib.request.urlopen("http://127.0.0.1:3000/health", timeout=10) as response:
    dashboard = json.loads(response.read().decode())
if dashboard.get("ok") is not True:
    raise SystemExit(f"dashboard health is not ok: {{dashboard}}")
PY

cd "$RELEASE"
set -a
. "$CONFIG/bot.env"
set +a
PREFLIGHT_JSON="$("$RELEASE/target/release/buba-paint" live-preflight)"
printf '%s' "$PREFLIGHT_JSON" | python3 -c 'import json,sys; data=json.load(sys.stdin); raise SystemExit(0 if data.get("ok") is True else f"live-preflight not ok: {{data}}")'

if [ "$(sqlite3 "$DB" 'PRAGMA quick_check;')" != "ok" ]; then
  echo "sqlite quick_check failed" >&2
  exit 31
fi
LIVE_TRADING_SESSIONS=$(sqlite3 "$DB" "SELECT COUNT(*) FROM live_sessions WHERE execution_mode='live_trading';")
if [ "$LIVE_TRADING_SESSIONS" != "0" ]; then
  echo "unexpected live_trading sessions: $LIVE_TRADING_SESSIONS" >&2
  exit 32
fi
ORDER_ROWS=$(sqlite3 "$DB" "SELECT (SELECT COUNT(*) FROM live_order_intents) + (SELECT COUNT(*) FROM live_orders) + (SELECT COUNT(*) FROM live_fills) + (SELECT COUNT(*) FROM live_redemptions);")
if [ "$ORDER_ROWS" != "0" ]; then
  echo "unexpected live order/fill/redemption rows: $ORDER_ROWS" >&2
  exit 33
fi
ROOT_DB_GARBAGE=$(find "$RELEASE" -maxdepth 1 \\( -name '*.db' -o -name '*.db-wal' -o -name '*.db-shm' \\) -print | wc -l | tr -d ' ')
if [ "$ROOT_DB_GARBAGE" != "0" ]; then
  echo "release root contains db garbage" >&2
  exit 34
fi
echo "acceptance_check=ok"
""",
        )

    def run_soak_sleep(self) -> None:
        """Wait for the requested soak duration while collecting periodic snapshots."""
        self.manifest["started_at_utc"] = utc_now()
        write_json(self.output_dir / "manifest.json", self.manifest)
        remaining = self.args.duration_seconds
        iteration = 1
        while remaining > 0:
            sleep_for = min(self.args.poll_seconds, remaining)
            if self.args.dry_run:
                remaining -= sleep_for
                continue
            time.sleep(sleep_for)
            remaining -= sleep_for
            self.capture_standard_evidence(f"poll-{iteration:02d}")
            iteration += 1
        self.manifest["finished_at_utc"] = utc_now()
        write_json(self.output_dir / "manifest.json", self.manifest)

    def write_notes(self) -> None:
        """Write a short human-readable evidence note."""
        notes = [
            "# Replay-Grade Readonly Soak 001",
            "",
            f"- Host: `{self.args.host}`",
            f"- Release: `{self.release_dir}`",
            f"- Runtime: `{self.runtime_dir}`",
            f"- Started: `{self.manifest.get('started_at_utc')}`",
            f"- Finished: `{self.manifest.get('finished_at_utc')}`",
            f"- Duration seconds: `{self.args.duration_seconds}`",
            "- Scope: no-order `live_readonly` soak. No `live_trading`, arming, orders, cancels, redemptions, write smoke, or canary.",
            "- Review `manifest.json`, `replay-quality.txt`, `live-preflight.json`, `dashboard-trading-summary.json`, and log-tail files before using this evidence for the next phase.",
            "",
        ]
        write_text(self.output_dir / "notes.md", "\n".join(notes))

    def run(self) -> int:
        """Execute the host soak."""
        try:
            if not self.args.dry_run:
                self.ensure_local_artifacts()
            self.create_remote_dirs()
            self.rsync_release()
            self.configure_remote_release()
            self.start_services()
            self.run_preflight()
            self.start_monitor_services()
            self.capture_standard_evidence("pre-soak")
            self.capture_dashboard_summary()
            self.run_soak_sleep()
            self.capture_standard_evidence("post-soak")
            self.run_replay_quality()
            self.run_acceptance_check()
            self.capture_ssh(
                "remote-env-report",
                f"set -euo pipefail; cat {shlex.quote(self.runtime_dir)}/env-report.json",
                "env-report.json",
            )
            if not self.args.skip_stop:
                self.stop_services()
                self.capture_standard_evidence("post-stop")
            self.write_notes()
            self.manifest["passed"] = True
            self.manifest["finished_at_utc"] = self.manifest.get("finished_at_utc") or utc_now()
            write_json(self.output_dir / "manifest.json", self.manifest)
            return 0
        except Exception as error:
            self.manifest["passed"] = False
            self.manifest["failure"] = str(error)
            self.manifest["finished_at_utc"] = utc_now()
            write_json(self.output_dir / "manifest.json", self.manifest)
            if not self.args.skip_stop and not self.args.dry_run:
                try:
                    self.stop_services()
                except Exception:
                    pass
            print(f"host soak failed: {error}", file=sys.stderr)
            return 1


def main() -> int:
    """Run the host readonly soak."""
    args = parse_args()
    if args.duration_seconds < 1:
        raise SystemExit("--duration-seconds must be positive")
    if args.poll_seconds < 1:
        raise SystemExit("--poll-seconds must be positive")
    runner = HostSoakRunner(args)
    return runner.run()


if __name__ == "__main__":
    raise SystemExit(main())
