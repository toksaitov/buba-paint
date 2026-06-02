#!/usr/bin/env python3
"""Deploy inventory-driven Docker Compose stacks to configured machines."""

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
import tomllib
from pathlib import Path
from typing import Any

from research_images import all_image_input_hashes

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_INVENTORY = REPO_ROOT / "ops" / "research-machines.toml"
LOCK_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
RESEARCH_LOCK_IMAGE_ENVS = {
    "dashboard": "BUBA_DASHBOARD_IMAGE",
    "research_worker": "BUBA_RESEARCH_WORKER_IMAGE",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    parser.add_argument("--machine", required=True)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--skip-sync", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--image-lock-override", type=Path)
    parser.add_argument("--allow-stale-image-lock", action="store_true")
    return parser.parse_args()


def load_inventory(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def machine_plan(
    inventory: dict[str, Any],
    machine_id: str,
    *,
    image_lock_override: Path | None = None,
    allow_stale_image_lock: bool = False,
) -> dict[str, Any]:
    machines = inventory.get("machines", {})
    if machine_id not in machines:
        raise KeyError(f"unknown machine '{machine_id}'")
    defaults = inventory.get("defaults", {})
    images = inventory.get("images", {})
    machine = machines[machine_id]
    registry = machine.get("registry", defaults.get("registry", ""))
    image_tag = defaults.get("image_tag", "local")
    registry_pinned = bool(machine.get("registry_pinned", False))
    if image_lock_override and not registry_pinned:
        raise ValueError("image lock override is supported only for registry-pinned machines")
    image_lock_file = str(image_lock_override) if image_lock_override else machine.get("image_lock_file")
    image_lock = (
        load_image_lock(image_lock_file, allow_stale=allow_stale_image_lock)
        if registry_pinned
        else None
    )
    return {
        "machine": machine_id,
        "role": machine["role"],
        "ssh_alias": machine["ssh_alias"],
        "mode": machine["mode"],
        "remote_root": machine["remote_root"],
        "compose_files": machine["compose_files"],
        "services": machine["services"],
        "execution_environment": machine.get("execution_environment", "ssh"),
        "wsl_distro": machine.get("wsl_distro"),
        "build_strategy": machine.get("build_strategy", "remote"),
        "registry_pinned": registry_pinned,
        "registry": registry,
        "image_tag": image_tag,
        "images": images,
        "image_lock_file": image_lock_file,
        "image_lock_override": str(image_lock_override) if image_lock_override else None,
        "allow_stale_image_lock": allow_stale_image_lock,
        "locked_images": locked_images_from_lock(image_lock) if image_lock else {},
        "registry_namespace": image_lock.get("namespace") if image_lock else None,
        "deferred": bool(machine.get("deferred", False)),
        "deferred_reason": machine.get("deferred_reason"),
        "will_connect": not bool(machine.get("deferred", False)),
    }


def load_image_lock(path_value: str | None, *, allow_stale: bool = False) -> dict[str, Any]:
    if not path_value:
        raise ValueError("registry-pinned machines require image_lock_file")
    path = Path(path_value)
    if not path.is_absolute():
        path = REPO_ROOT / path
    with path.open("rb") as handle:
        lock = json.load(handle)
    validate_image_lock(lock, path, allow_stale=allow_stale)
    return lock


def validate_image_lock(lock: dict[str, Any], path: Path, *, allow_stale: bool = False) -> None:
    images = lock.get("images")
    if not isinstance(images, dict):
        raise ValueError(f"{path} missing images object")
    missing = sorted(set(RESEARCH_LOCK_IMAGE_ENVS) - set(images))
    if missing:
        raise ValueError(f"{path} missing image refs for: {', '.join(missing)}")
    current_hashes = all_image_input_hashes(REPO_ROOT)
    locked_hashes = lock.get("input_hashes")
    if locked_hashes != current_hashes and not allow_stale:
        raise ValueError(f"{path} is stale; run scripts/publish-research-images.py")
    for key in RESEARCH_LOCK_IMAGE_ENVS:
        image = images.get(key)
        if not isinstance(image, dict):
            raise ValueError(f"{path} image {key} must be an object")
        ref = image.get("ref")
        digest = image.get("digest")
        if not isinstance(ref, str) or "@sha256:" not in ref:
            raise ValueError(f"{path} image {key} must contain a digest-pinned ref")
        if not isinstance(digest, str) or not LOCK_DIGEST_RE.fullmatch(digest):
            raise ValueError(f"{path} image {key} has invalid digest")


def locked_images_from_lock(lock: dict[str, Any]) -> dict[str, str]:
    return {
        key: lock["images"][key]["ref"]
        for key in sorted(RESEARCH_LOCK_IMAGE_ENVS)
    }


def quote(value: str) -> str:
    return shlex.quote(value)


def run(cmd: list[str], *, input_bytes: bytes | None = None) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(cmd, input=input_bytes, check=True)


def supported_tar_metadata_flags() -> list[str]:
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


def remote_stdin_command(plan: dict[str, Any], command: str) -> list[str]:
    ssh_alias = plan["ssh_alias"]
    environment = plan.get("execution_environment", "ssh")
    if environment == "wsl":
        distro = plan.get("wsl_distro")
        if not distro:
            raise ValueError("wsl execution_environment requires wsl_distro")
        return ["ssh", ssh_alias, f"wsl -d {quote(distro)} -- sh -c {quote(command)}"]
    if environment == "ssh":
        return ["ssh", ssh_alias, f"sh -c {quote(command)}"]
    raise ValueError(f"unsupported execution_environment: {environment}")


def remote_shell_command(plan: dict[str, Any]) -> list[str]:
    ssh_alias = plan["ssh_alias"]
    environment = plan.get("execution_environment", "ssh")
    if environment == "wsl":
        distro = plan.get("wsl_distro")
        if not distro:
            raise ValueError("wsl execution_environment requires wsl_distro")
        return ["ssh", ssh_alias, f"wsl -d {quote(distro)} -- bash -se"]
    if environment == "ssh":
        return ["ssh", ssh_alias, "bash -se"]
    raise ValueError(f"unsupported execution_environment: {environment}")


def remote_run(plan: dict[str, Any], script: str) -> None:
    run(remote_shell_command(plan), input_bytes=prepare_remote_script(plan, script).encode("utf-8"))


def remote_output(plan: dict[str, Any], script: str) -> str:
    result = subprocess.run(
        remote_shell_command(plan),
        input=prepare_remote_script(plan, script).encode("utf-8"),
        check=True,
        capture_output=True,
    )
    return result.stdout.decode("utf-8", errors="replace")


def prepare_remote_script(plan: dict[str, Any], script: str) -> str:
    if plan.get("execution_environment") != "wsl":
        return script
    return "\n".join(
        [
            "if [ -x /Docker/host/bin/docker ]; then export PATH=/Docker/host/bin:$PATH; fi",
            script,
        ]
    )


def compose_args(plan: dict[str, Any]) -> str:
    return " ".join(f"-f {quote(path)}" for path in plan["compose_files"])


def compose_image_exports(plan: dict[str, Any]) -> str:
    locked_images = plan.get("locked_images") or {}
    lines = []
    for key, env_name in RESEARCH_LOCK_IMAGE_ENVS.items():
        image = locked_images.get(key)
        if not image:
            raise ValueError(f"registry-pinned deploy missing locked image for {key}")
        lines.append(f"export {env_name}={quote(image)}")
    return "\n".join(lines)


def docker_config_setup(plan: dict[str, Any]) -> str:
    if plan.get("registry_pinned") or plan.get("registry"):
        return ""
    root = plan["remote_root"]
    return "\n".join(
        [
            f"docker_config={quote(root)}/.docker/docker-config",
            "mkdir -p \"$docker_config\"",
            "if [ ! -f \"$docker_config/config.json\" ]; then",
            "  printf '{}\\n' > \"$docker_config/config.json\"",
            "fi",
            "export DOCKER_CONFIG=\"$docker_config\"",
        ]
    )


def gh_scopes() -> set[str]:
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
    scopes = gh_scopes()
    if "read:packages" not in scopes and "write:packages" not in scopes:
        raise RuntimeError(
            "gh token lacks read:packages; run `gh auth refresh -s read:packages`"
        )
    result = subprocess.run(
        ["gh", "auth", "token"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def ensure_research_secrets(plan: dict[str, Any]) -> None:
    uid_gid = remote_output(plan, "id -u; id -g").strip().splitlines()
    uid = uid_gid[0] if len(uid_gid) >= 1 else "1000"
    gid = uid_gid[1] if len(uid_gid) >= 2 else "1000"
    admin_password = secrets.token_hex(18)
    jwt_secret = secrets.token_hex(32)
    worker_token = secrets.token_hex(32)
    env_text = "\n".join(
        [
            f"BUBA_UID={uid}",
            f"BUBA_GID={gid}",
            "ADMIN_USER=admin",
            f"ADMIN_PASSWORD={admin_password}",
            f"BUBA_RESEARCH_WORKER_TOKEN={worker_token}",
            "BUBA_RESEARCH_CONTROLLER_URL=http://research-dashboard:3001",
            "BUBA_RESEARCH_MACHINE_ID=research",
            "BUBA_RESEARCH_WORKER_ID=research-worker-testing",
            "BUBA_RESEARCH_HEARTBEAT_MS=30000",
            "BUBA_RESEARCH_TRANSFER_STALE_MS=1800000",
            "BUBA_RESEARCH_DASHBOARD_PORT=3002",
            "BUBA_RESEARCH_RUNTIME_DIR=./.docker/research/runtime",
            "BUBA_RESEARCH_WORK_DIR=./.docker/research/work",
            "BUBA_RESEARCH_SSH_DIR=/home/testing/.ssh",
            "BUBA_DASHBOARD_CONFIG_DIR=./.docker/research/config",
            "",
        ]
    )
    dashboard_config = "\n".join(
        [
            "[server]",
            "port = 3001",
            f'jwt_secret = "{jwt_secret}"',
            "",
        ]
    )
    env_b64 = base64.b64encode(env_text.encode("utf-8")).decode("ascii")
    config_b64 = base64.b64encode(dashboard_config.encode("utf-8")).decode("ascii")
    root = plan["remote_root"]
    remote_run(
        plan,
        "\n".join(
            [
                "set -euo pipefail",
                f"root={quote(root)}",
                "mkdir -p \"$root/.docker/research/config\" \"$root/.docker/research/runtime\" \"$root/.docker/research/work\"",
                "if [ ! -f \"$root/.env\" ]; then",
                f"  printf '%s' {quote(env_b64)} | base64 -d > \"$root/.env\"",
                "  chmod 600 \"$root/.env\"",
                "fi",
                f"ensure_env_key() {{ key=\"$1\"; value=\"$2\"; if ! grep -q \"^${{key}}=\" \"$root/.env\"; then printf '%s=%s\\n' \"$key\" \"$value\" >> \"$root/.env\"; fi; }}",
                f"ensure_env_key BUBA_RESEARCH_WORKER_TOKEN {quote(worker_token)}",
                "ensure_env_key BUBA_RESEARCH_CONTROLLER_URL http://research-dashboard:3001",
                "ensure_env_key BUBA_RESEARCH_MACHINE_ID research",
                "ensure_env_key BUBA_RESEARCH_WORKER_ID research-worker-testing",
                "ensure_env_key BUBA_RESEARCH_HEARTBEAT_MS 30000",
                "ensure_env_key BUBA_RESEARCH_TRANSFER_STALE_MS 1800000",
                "ensure_env_key BUBA_RESEARCH_SSH_DIR /home/testing/.ssh",
                "ensure_env_key BUBA_DASHBOARD_CONFIG_DIR ./.docker/research/config",
                "if [ ! -f \"$root/.docker/research/config/dashboard.toml\" ]; then",
                f"  printf '%s' {quote(config_b64)} | base64 -d > \"$root/.docker/research/config/dashboard.toml\"",
                "  chmod 600 \"$root/.docker/research/config/dashboard.toml\"",
                "fi",
            ]
        ),
    )


def sync_repo(plan: dict[str, Any]) -> None:
    root = plan["remote_root"]
    archive_path = f"/tmp/buba-paint-sync-{secrets.token_hex(8)}.tar.gz"
    excludes = [
        "--exclude=.git",
        "--exclude=target",
        "--exclude=.docker",
        "--exclude=.env",
        "--exclude=data",
        "--exclude=node_modules",
        "--exclude=dashboard/client/node_modules",
        "--exclude=polymarket-sidecar/node_modules",
        "--exclude=runs",
    ]
    remote_script = "\n".join(
        [
            "set -euo pipefail",
            f"root={quote(root)}",
            f"archive={quote(archive_path)}",
            "trap 'rm -f \"$archive\"' EXIT",
            "mkdir -p \"$root\"",
            "cd \"$root\"",
            "find . -mindepth 1 -maxdepth 1 ! -name .docker ! -name .env -exec rm -rf -- {} +",
            "tar -xzf \"$archive\" -C \"$root\"",
        ]
    )
    tar_cmd = ["tar", *supported_tar_metadata_flags(), *excludes, "-czf", "-", "."]
    tar_env = {**os.environ, "COPYFILE_DISABLE": "1"}
    tar_proc = subprocess.Popen(tar_cmd, cwd=REPO_ROOT, stdout=subprocess.PIPE, env=tar_env)
    try:
        assert tar_proc.stdout is not None
        ssh_proc = subprocess.run(
            remote_stdin_command(plan, f"cat > {quote(archive_path)}"),
            stdin=tar_proc.stdout,
        )
        tar_proc.stdout.close()
        tar_status = tar_proc.wait()
        if ssh_proc.returncode != 0:
            raise subprocess.CalledProcessError(ssh_proc.returncode, ssh_proc.args)
        if tar_status != 0:
            raise subprocess.CalledProcessError(tar_status, tar_cmd)
        remote_run(plan, remote_script)
    finally:
        if tar_proc.poll() is None:
            tar_proc.kill()


def sync_deploy_files(plan: dict[str, Any]) -> None:
    root = plan["remote_root"]
    archive_path = f"/tmp/buba-paint-deploy-files-{secrets.token_hex(8)}.tar.gz"
    paths = [str(path) for path in plan["compose_files"]]
    remote_script = "\n".join(
        [
            "set -euo pipefail",
            f"root={quote(root)}",
            f"archive={quote(archive_path)}",
            "trap 'rm -f \"$archive\"' EXIT",
            "mkdir -p \"$root\"",
            "tar -xzf \"$archive\" -C \"$root\"",
        ]
    )
    tar_cmd = ["tar", *supported_tar_metadata_flags(), "-czf", "-", *paths]
    tar_env = {**os.environ, "COPYFILE_DISABLE": "1"}
    tar_proc = subprocess.Popen(tar_cmd, cwd=REPO_ROOT, stdout=subprocess.PIPE, env=tar_env)
    try:
        assert tar_proc.stdout is not None
        ssh_proc = subprocess.run(
            remote_stdin_command(plan, f"cat > {quote(archive_path)}"),
            stdin=tar_proc.stdout,
        )
        tar_proc.stdout.close()
        tar_status = tar_proc.wait()
        if ssh_proc.returncode != 0:
            raise subprocess.CalledProcessError(ssh_proc.returncode, ssh_proc.args)
        if tar_status != 0:
            raise subprocess.CalledProcessError(tar_status, tar_cmd)
        remote_run(plan, remote_script)
    finally:
        if tar_proc.poll() is None:
            tar_proc.kill()


def compose_up(plan: dict[str, Any], *, skip_build: bool) -> None:
    root = plan["remote_root"]
    services = " ".join(quote(service) for service in plan["services"])
    build_flag = "" if skip_build else " --build"
    remote_run(
        plan,
        "\n".join(
            [
                "set -euo pipefail",
                f"cd {quote(root)}",
                docker_config_setup(plan),
                f"docker compose {compose_args(plan)} config --quiet",
                f"docker compose {compose_args(plan)} up -d{build_flag} {services}",
            ]
        ),
    )


def compose_up_registry_pinned(plan: dict[str, Any]) -> None:
    root = plan["remote_root"]
    services = " ".join(quote(service) for service in plan["services"])
    username = plan.get("registry_namespace") or "toksaitov"
    token_b64 = base64.b64encode(ghcr_pull_token().encode("utf-8")).decode("ascii")
    remote_run(
        plan,
        "\n".join(
            [
                "set -euo pipefail",
                f"cd {quote(root)}",
                "mkdir -p .docker",
                "docker_config=$(mktemp -d .docker/ghcr-auth.XXXXXX)",
                "cleanup() { rm -rf \"$docker_config\"; }",
                "trap cleanup EXIT",
                "export DOCKER_CONFIG=\"$docker_config\"",
                f"export GHCR_USER={quote(username)}",
                f"export GHCR_TOKEN_B64={quote(token_b64)}",
                "python3 - <<'PY'",
                "import base64",
                "import json",
                "import os",
                "from pathlib import Path",
                "user = os.environ['GHCR_USER']",
                "token = base64.b64decode(os.environ['GHCR_TOKEN_B64']).decode('utf-8')",
                "auth = base64.b64encode(f'{user}:{token}'.encode('utf-8')).decode('ascii')",
                "Path(os.environ['DOCKER_CONFIG']).mkdir(parents=True, exist_ok=True)",
                "Path(os.environ['DOCKER_CONFIG'], 'config.json').write_text(",
                "    json.dumps({'auths': {'ghcr.io': {'auth': auth}}}),",
                "    encoding='utf-8',",
                ")",
                "PY",
                "unset GHCR_TOKEN_B64",
                compose_image_exports(plan),
                f"docker compose {compose_args(plan)} config --quiet",
                f"docker compose {compose_args(plan)} pull {services}",
                f"docker compose {compose_args(plan)} up -d --no-build {services}",
            ]
        ),
    )


def verify_research(plan: dict[str, Any]) -> dict[str, Any]:
    root = plan["remote_root"]
    script = "\n".join(
        [
            "set -euo pipefail",
            f"cd {quote(root)}",
            "python3 - <<'PY'",
            "import json, subprocess, urllib.request",
            "def run(args):",
            "    return subprocess.run(args, capture_output=True, text=True, check=False)",
            "def tail(path):",
            "    result = run(['tail', '-n', '20', path])",
            "    return result.stdout if result.returncode == 0 else ''",
            "ps = run(['docker', 'compose', '-f', 'docker-compose.research.yml', 'ps', '--format', 'json'])",
            "rows = []",
            "for line in ps.stdout.splitlines():",
            "    if not line.strip():",
            "        continue",
            "    row = json.loads(line)",
            "    rows.append({key: row.get(key) for key in ('Service', 'Name', 'ID', 'Image', 'State', 'Status', 'Health', 'RunningFor') if key in row})",
            "with urllib.request.urlopen('http://localhost:3002/health', timeout=10) as response:",
            "    health = json.loads(response.read())",
            "print(json.dumps({",
            "    'compose_ps': {'returncode': ps.returncode, 'services': rows, 'stderr': ps.stderr},",
            "    'health': health,",
            "    'worker_log_tail': tail('.docker/research/runtime/research-worker.log'),",
            "    'dashboard_log_tail': tail('.docker/research/runtime/dashboard.log'),",
            "}, indent=2, sort_keys=True))",
            "PY",
        ]
    )
    output = remote_output(plan, script)
    try:
        return {"verification": json.loads(output)}
    except json.JSONDecodeError:
        return {"verification": output}


def collect_failure_diagnostics(plan: dict[str, Any]) -> dict[str, Any]:
    script = "\n".join(
        [
            "set +e",
            f"cd {quote(plan['remote_root'])} 2>/dev/null || true",
            "python3 - <<'PY'",
            "import json, subprocess",
            "def capture(cmd):",
            "    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)",
            "    return {'returncode': result.returncode, 'stdout': result.stdout[-12000:], 'stderr': result.stderr[-4000:]}",
            "payload = {",
            "    'compose_ps': capture('docker compose -f docker-compose.research.yml ps --format json'),",
            "    'health': capture('curl -sf http://localhost:3002/health'),",
            "    'worker_log_tail': capture('tail -n 80 .docker/research/runtime/research-worker.log 2>/dev/null'),",
            "    'dashboard_log_tail': capture('tail -n 80 .docker/research/runtime/dashboard.log 2>/dev/null'),",
            "}",
            "print(json.dumps(payload, indent=2, sort_keys=True))",
            "PY",
        ]
    )
    try:
        return {"ok": True, "diagnostics": json.loads(remote_output(plan, script))}
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as exc:
        return {"ok": False, "error": str(exc)}


def deploy(plan: dict[str, Any], *, skip_sync: bool, skip_build: bool) -> dict[str, Any]:
    if plan["deferred"]:
        raise RuntimeError(plan.get("deferred_reason") or "machine is deferred")
    if plan["machine"] != "research":
        raise RuntimeError("non-dry-run deployment is currently enabled only for research")

    remote_run(plan, "command -v docker >/dev/null && docker compose version >/dev/null")
    ensure_research_secrets(plan)
    if not skip_sync:
        if plan.get("registry_pinned"):
            sync_deploy_files(plan)
        else:
            sync_repo(plan)
    if plan.get("registry_pinned"):
        compose_up_registry_pinned(plan)
    else:
        compose_up(plan, skip_build=skip_build)
    return verify_research(plan)


def main() -> int:
    args = parse_args()
    try:
        inventory = load_inventory(args.inventory)
        plan = machine_plan(
            inventory,
            args.machine,
            image_lock_override=args.image_lock_override,
            allow_stale_image_lock=args.allow_stale_image_lock,
        )
    except (OSError, KeyError, ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2

    if args.dry_run:
        print(json.dumps(plan, indent=2, sort_keys=True))
        return 0

    try:
        result = deploy(plan, skip_sync=args.skip_sync, skip_build=args.skip_build)
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as exc:
        print(str(exc), file=sys.stderr)
        if plan.get("machine") == "research" and plan.get("will_connect"):
            diagnostics = collect_failure_diagnostics(plan)
            print(json.dumps({"failure_diagnostics": diagnostics}, indent=2, sort_keys=True), file=sys.stderr)
        return 2
    print(json.dumps({"plan": plan, "result": result}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
