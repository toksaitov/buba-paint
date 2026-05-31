#!/usr/bin/env python3
"""Deploy live observability services without starting bot services."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import secrets
import shlex
import subprocess
import sys
from pathlib import Path
from typing import Any

from research_images import all_image_input_hashes

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK_FILE = REPO_ROOT / "ops" / "live-images.lock.json"
LIVE_IMAGE_ENVS = {
    "dashboard": "BUBA_DASHBOARD_IMAGE",
    "agent": "BUBA_AGENT_IMAGE",
    "paint": "BUBA_PAINT_IMAGE",
    "sidecar": "BUBA_SIDECAR_IMAGE",
}
COMPOSE_FILES = [
    "docker-compose.yml",
    "docker-compose.live-readonly.yml",
    "docker-compose.prod.yml",
    "docker-compose.live-stopped.yml",
]
OBSERVABILITY_SERVICES = ("agent", "dashboard", "caddy")
BOT_SERVICES = ("paint", "sidecar")
LOCK_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
ALLOWED_DIRTY_PATHS = {"ops/live-images.lock.json", "ops/research-images.lock.json"}


def parse_args() -> argparse.Namespace:
    """Parse CLI arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="buba-paint")
    parser.add_argument("--domain", default="buba.toksaitov.com")
    parser.add_argument("--remote-root", default="~/buba-paint-live")
    parser.add_argument("--lock-file", type=Path, default=DEFAULT_LOCK_FILE)
    parser.add_argument("--expected-runtime-name", default="live-readonly-20260514-184119")
    parser.add_argument("--expected-db-sha256", required=False)
    parser.add_argument("--allow-dirty-source", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def quote(value: str) -> str:
    """Return a shell-quoted string."""
    return shlex.quote(value)


def run(cmd: list[str], *, input_bytes: bytes | None = None) -> subprocess.CompletedProcess[bytes]:
    """Run a local command and fail on non-zero exit."""
    return subprocess.run(cmd, input=input_bytes, cwd=REPO_ROOT, check=True)


def output(cmd: list[str]) -> str:
    """Run a local command and return stdout."""
    result = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def remote_output(host: str, script: str) -> str:
    """Run a remote shell script and return stdout."""
    result = subprocess.run(
        ["ssh", host, "bash -se"],
        input=script,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        message = [f"remote command failed on {host} with exit {result.returncode}"]
        if result.stdout:
            message.append("stdout:\n" + result.stdout)
        if result.stderr:
            message.append("stderr:\n" + result.stderr)
        raise RuntimeError("\n".join(message))
    return result.stdout


def remote_home(host: str) -> str:
    """Return the remote home directory."""
    return remote_output(host, 'printf "%s" "$HOME"\n').strip()


def expand_remote_root(host: str, remote_root: str) -> str:
    """Expand a leading tilde in a remote path."""
    if remote_root.startswith("~/"):
        return f"{remote_home(host)}{remote_root[1:]}"
    return remote_root


def supported_tar_metadata_flags() -> list[str]:
    """Return tar metadata flags supported on the local platform."""
    supported = []
    for option in ("--no-xattrs", "--no-fflags"):
        result = subprocess.run(
            ["tar", option, "-cf", os.devnull, "--files-from", os.devnull],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode == 0:
            supported.append(option)
    return supported


def sync_compose_files(host: str, release: str) -> None:
    """Sync only Compose files required by the stopped-live deployment."""
    archive_path = f"/tmp/buba-live-stopped-{secrets.token_hex(8)}.tar.gz"
    tar_cmd = ["tar", *supported_tar_metadata_flags(), "-czf", "-", *COMPOSE_FILES]
    remote_script = "\n".join(
        [
            "set -euo pipefail",
            f"release={quote(release)}",
            f"archive={quote(archive_path)}",
            "trap 'rm -f \"$archive\"' EXIT",
            "mkdir -p \"$release\"",
            "tar -xzf \"$archive\" -C \"$release\"",
        ]
    )
    tar_env = {**os.environ, "COPYFILE_DISABLE": "1"}
    tar_proc = subprocess.Popen(tar_cmd, cwd=REPO_ROOT, stdout=subprocess.PIPE, env=tar_env)
    try:
        assert tar_proc.stdout is not None
        ssh_proc = subprocess.run(["ssh", host, f"cat > {quote(archive_path)}"], stdin=tar_proc.stdout)
        tar_proc.stdout.close()
        tar_status = tar_proc.wait()
        if ssh_proc.returncode != 0:
            raise subprocess.CalledProcessError(ssh_proc.returncode, ssh_proc.args)
        if tar_status != 0:
            raise subprocess.CalledProcessError(tar_status, tar_cmd)
        remote_output(host, remote_script)
    finally:
        if tar_proc.poll() is None:
            tar_proc.kill()


def load_image_lock(path: Path) -> dict[str, Any]:
    """Load and validate the live image lock."""
    lock = json.loads(path.read_text(encoding="utf-8"))
    validate_image_lock(lock, path)
    return lock


def validate_image_lock(lock: dict[str, Any], path: Path) -> None:
    """Validate live image refs and input hashes."""
    images = lock.get("images")
    if not isinstance(images, dict):
        raise ValueError(f"{path} missing images object")
    missing = sorted(set(LIVE_IMAGE_ENVS) - set(images))
    if missing:
        raise ValueError(f"{path} missing image refs for: {', '.join(missing)}")
    current_hashes = all_image_input_hashes(REPO_ROOT, tuple(LIVE_IMAGE_ENVS))
    if lock.get("input_hashes") != current_hashes:
        raise ValueError(f"{path} is stale; run scripts/publish-live-images.py")
    for key in LIVE_IMAGE_ENVS:
        image = images.get(key)
        if not isinstance(image, dict):
            raise ValueError(f"{path} image {key} must be an object")
        ref = image.get("ref")
        digest = image.get("digest")
        if not isinstance(ref, str) or "@sha256:" not in ref:
            raise ValueError(f"{path} image {key} must contain a digest-pinned ref")
        if not isinstance(digest, str) or not LOCK_DIGEST_RE.fullmatch(digest):
            raise ValueError(f"{path} image {key} has invalid digest")


def locked_image_exports(lock: dict[str, Any]) -> str:
    """Return shell exports for digest-pinned live images."""
    lines = []
    for key, env_name in LIVE_IMAGE_ENVS.items():
        lines.append(f"export {env_name}={quote(lock['images'][key]['ref'])}")
    return "\n".join(lines)


def gh_scopes() -> set[str]:
    """Return scopes for the current gh token."""
    result = subprocess.run(
        ["gh", "api", "-i", "/user"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    scopes: set[str] = set()
    for line in result.stdout.splitlines():
        if line.lower().startswith("x-oauth-scopes:"):
            value = line.split(":", 1)[1]
            scopes.update(scope.strip() for scope in value.split(",") if scope.strip())
    return scopes


def ghcr_pull_token() -> str:
    """Return a gh token that can pull private GHCR images."""
    scopes = gh_scopes()
    if "read:packages" not in scopes and "write:packages" not in scopes:
        raise RuntimeError(
            "gh token lacks read:packages; run `gh auth refresh -s read:packages`"
        )
    return output(["gh", "auth", "token"])


def ensure_clean_source(*, allow_dirty: bool) -> list[str]:
    """Fail when source files other than image locks are dirty."""
    status = output(["git", "status", "--porcelain"])
    dirty = []
    for line in status.splitlines():
        path = line[3:] if len(line) > 3 else line
        if path not in ALLOWED_DIRTY_PATHS:
            dirty.append(line)
    if dirty and not allow_dirty:
        raise RuntimeError(
            "refusing stopped-live deploy from dirty source files:\n" + "\n".join(dirty)
        )
    return dirty


def dry_run_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Return the dry-run deployment plan."""
    lock_status: dict[str, Any]
    try:
        lock = load_image_lock(args.lock_file)
        lock_status = {
            "ok": True,
            "commit": lock.get("commit"),
            "tag": lock.get("tag"),
            "images": {key: lock["images"][key]["ref"] for key in sorted(LIVE_IMAGE_ENVS)},
        }
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        lock_status = {"ok": False, "error": str(exc)}
    return {
        "host": args.host,
        "domain": args.domain,
        "remote_root": args.remote_root,
        "compose_files": COMPOSE_FILES,
        "pull_services": list(BOT_SERVICES + OBSERVABILITY_SERVICES[:2]),
        "up_services": list(OBSERVABILITY_SERVICES),
        "bot_services_must_remain_stopped": list(BOT_SERVICES),
        "expected_runtime_name": args.expected_runtime_name,
        "expected_db_sha256": args.expected_db_sha256,
        "allow_dirty_source": args.allow_dirty_source,
        "lock": lock_status,
    }


def preflight_remote_script(root: str, expected_runtime_name: str, expected_db_sha256: str) -> str:
    """Return the remote preflight script."""
    return "\n".join(
        [
            "set -euo pipefail",
            f"root={quote(root)}",
            f"expected_runtime_name={quote(expected_runtime_name)}",
            f"expected_db_sha256={quote(expected_db_sha256)}",
            "current=\"$root/current\"",
            "test -d \"$current\"",
            "test -f \"$current/.env\"",
            "set -a",
            ". \"$current/.env\"",
            "set +a",
            "runtime_name=$(basename \"$BUBA_RUNTIME_DIR\")",
            "test \"$runtime_name\" = \"$expected_runtime_name\"",
            "test -f \"$BUBA_RUNTIME_DIR/paint.db\"",
            "actual_sha=$(sha256sum \"$BUBA_RUNTIME_DIR/paint.db\" | awk '{print $1}')",
            "test \"$actual_sha\" = \"$expected_db_sha256\"",
            "running_bot=$(sudo docker ps --format '{{.Names}} {{.State}}' | awk '$1 ~ /^buba-paint-(paint|sidecar)-1$/ {print}')",
            "test -z \"$running_bot\"",
            "python3 - <<'PY'",
            "import json, os",
            "print(json.dumps({",
            "  'runtime_dir': os.environ['BUBA_RUNTIME_DIR'],",
            "  'db_path': os.environ['BUBA_RUNTIME_DIR'] + '/paint.db',",
            "  'runtime_name': os.path.basename(os.environ['BUBA_RUNTIME_DIR']),",
            "}, indent=2, sort_keys=True))",
            "PY",
        ]
    )


def deploy_remote_script(root: str, release: str, lock: dict[str, Any], token: str) -> str:
    """Return the remote stopped-live deployment script."""
    token_b64 = base64.b64encode(token.encode("utf-8")).decode("ascii")
    compose = " ".join(f"-f {quote(path)}" for path in COMPOSE_FILES)
    pull_services = " ".join(quote(service) for service in (*BOT_SERVICES, "agent", "dashboard"))
    up_services = " ".join(quote(service) for service in OBSERVABILITY_SERVICES)
    docker_env = "sudo env DOCKER_CONFIG=\"$docker_config\" docker"
    compose_env_exports = " ".join(
        f"{env_name}=\"${env_name}\"" for env_name in LIVE_IMAGE_ENVS.values()
    )
    compose_cmd = f"sudo env DOCKER_CONFIG=\"$docker_config\" {compose_env_exports} docker compose --env-file .env {compose}"
    return "\n".join(
        [
            "set -euo pipefail",
            f"root={quote(root)}",
            f"release={quote(release)}",
            "current=\"$root/current\"",
            "test -f \"$current/.env\"",
            "release_env=\"$release/.env\"",
            "current_env=$(readlink -f \"$current/.env\")",
            "if [ ! -f \"$release_env\" ] || [ \"$current_env\" != \"$(readlink -f \"$release_env\")\" ]; then cp \"$current/.env\" \"$release_env\"; fi",
            "ln -sfn \"$release\" \"$root/current\"",
            "cd \"$release\"",
            "mkdir -p .docker",
            "docker_config=$(mktemp -d /tmp/buba-live-ghcr.XXXXXX)",
            f"cleanup() {{ {docker_env} logout ghcr.io >/dev/null 2>&1 || true; sudo rm -rf \"$docker_config\" >/dev/null 2>&1 || rm -rf \"$docker_config\"; }}",
            "trap cleanup EXIT",
            f"printf '%s' {quote(token_b64)} | base64 -d | {docker_env} login ghcr.io -u {quote(lock.get('namespace') or 'toksaitov')} --password-stdin >/dev/null",
            locked_image_exports(lock),
            f"{compose_cmd} config --quiet",
            f"{compose_cmd} pull {pull_services}",
            f"{compose_cmd} stop {' '.join(BOT_SERVICES)} >/dev/null 2>&1 || true",
            f"{compose_cmd} up -d --no-build --no-deps {up_services}",
        ]
    )


def verify_remote_script(root: str, domain: str, lock: dict[str, Any]) -> str:
    """Return the remote verification script."""
    compose = " ".join(f"-f {quote(path)}" for path in COMPOSE_FILES)
    expected_json = json.dumps({key: lock["images"][key]["ref"] for key in sorted(LIVE_IMAGE_ENVS)})
    return "\n".join(
        [
            "set -euo pipefail",
            f"root={quote(root)}",
            f"export verify_domain={quote(domain)}",
            f"export expected_json={quote(expected_json)}",
            "cd \"$root/current\"",
            "set -a",
            ". ./.env",
            "set +a",
            locked_image_exports(lock),
            "python3 - <<'PY'",
            "import json, os, subprocess",
            f"compose = {json.dumps(['sudo', 'env', *[f'{env}=${env}' for env in LIVE_IMAGE_ENVS.values()], 'docker', 'compose', '--env-file', '.env', *sum((['-f', path] for path in COMPOSE_FILES), [])])}",
            "compose = [os.path.expandvars(arg) for arg in compose]",
            "def run(args, check=False):",
            "    return subprocess.run(args, capture_output=True, text=True, check=check)",
            "ps = run([*compose, 'ps', '--all', '--format', 'json'], check=True)",
            "rows = [json.loads(line) for line in ps.stdout.splitlines() if line.strip()]",
            "running_bot = [row for row in rows if row.get('Service') in ('paint', 'sidecar') and row.get('State') == 'running']",
            "if running_bot:",
            "    raise SystemExit('bot services are running: ' + json.dumps(running_bot))",
            "agent = run([*compose, 'exec', '-T', 'agent', 'curl', '-fsS', 'http://localhost:9090/health'], check=True).stdout",
            "dashboard = run([*compose, 'exec', '-T', 'dashboard', 'curl', '-fsS', 'http://localhost:3001/health'], check=True).stdout",
            "public = run(['curl', '-fsS', '--max-time', '20', 'https://' + os.environ['verify_domain'] + '/health'], check=False)",
            "sha = run(['sha256sum', os.environ['BUBA_RUNTIME_DIR'] + '/paint.db'], check=True).stdout.split()[0]",
            "print(json.dumps({",
            "    'compose_rows': rows,",
            "    'agent_health': json.loads(agent),",
            "    'dashboard_health': json.loads(dashboard),",
            "    'db_sha256': sha,",
            "    'public_health_returncode': public.returncode,",
            "    'public_health_stdout': public.stdout,",
            "    'public_health_stderr': public.stderr,",
            "    'expected_images': json.loads(os.environ['expected_json']),",
            "}, indent=2, sort_keys=True))",
            "PY",
        ]
    )


def main() -> int:
    """Run the stopped-live deployment."""
    args = parse_args()
    try:
        if args.dry_run:
            print(json.dumps(dry_run_payload(args), indent=2, sort_keys=True))
            return 0
        if not args.expected_db_sha256:
            raise RuntimeError("--expected-db-sha256 is required for non-dry-run deploys")
        dirty_source = ensure_clean_source(allow_dirty=args.allow_dirty_source)
        lock = load_image_lock(args.lock_file)
        root = expand_remote_root(args.host, args.remote_root)
        preflight = json.loads(remote_output(args.host, preflight_remote_script(root, args.expected_runtime_name, args.expected_db_sha256)))
        release = f"{root}/releases/stopped-live-{output(['git', 'rev-parse', '--short', 'HEAD'])}"
        sync_compose_files(args.host, release)
        remote_output(args.host, deploy_remote_script(root, release, lock, ghcr_pull_token()))
        verification = json.loads(remote_output(args.host, verify_remote_script(root, args.domain, lock)))
        print(json.dumps({"dirty_source_files": dirty_source, "preflight": preflight, "release": release, "verification": verification}, indent=2, sort_keys=True))
        return 0
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError, json.JSONDecodeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
