#!/usr/bin/env python3
"""Deploy inventory-driven Docker Compose stacks to configured machines."""

from __future__ import annotations

import argparse
import base64
import json
import os
import secrets
import shlex
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_INVENTORY = REPO_ROOT / "ops" / "research-machines.toml"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    parser.add_argument("--machine", required=True)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--skip-sync", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    return parser.parse_args()


def load_inventory(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def machine_plan(inventory: dict[str, Any], machine_id: str) -> dict[str, Any]:
    machines = inventory.get("machines", {})
    if machine_id not in machines:
        raise KeyError(f"unknown machine '{machine_id}'")
    defaults = inventory.get("defaults", {})
    images = inventory.get("images", {})
    machine = machines[machine_id]
    registry = defaults.get("registry", "")
    image_tag = defaults.get("image_tag", "local")
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
        "registry_pinned": bool(machine.get("registry_pinned", False)),
        "registry": registry,
        "image_tag": image_tag,
        "images": images,
        "deferred": bool(machine.get("deferred", False)),
        "deferred_reason": machine.get("deferred_reason"),
        "will_connect": not bool(machine.get("deferred", False)),
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
    run(remote_shell_command(plan), input_bytes=script.encode("utf-8"))


def remote_output(plan: dict[str, Any], script: str) -> str:
    result = subprocess.run(
        remote_shell_command(plan),
        input=script.encode("utf-8"),
        check=True,
        capture_output=True,
    )
    return result.stdout.decode("utf-8", errors="replace")


def compose_args(plan: dict[str, Any]) -> str:
    return " ".join(f"-f {quote(path)}" for path in plan["compose_files"])


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
            "BUBA_DASHBOARD_CONFIG=./.docker/research/config/dashboard.toml",
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
                f"docker compose {compose_args(plan)} config --quiet",
                f"docker compose {compose_args(plan)} up -d{build_flag} {services}",
            ]
        ),
    )


def verify_research(plan: dict[str, Any]) -> dict[str, Any]:
    root = plan["remote_root"]
    script = "\n".join(
        [
            "set -euo pipefail",
            f"cd {quote(root)}",
            "docker compose -f docker-compose.research.yml ps --format json",
            "printf '\\n--- health ---\\n'",
            "curl -sf http://localhost:3002/health",
            "printf '\\n--- worker-log ---\\n'",
            "tail -n 20 .docker/research/runtime/research-worker.log 2>/dev/null || true",
            "printf '\\n--- dashboard-log ---\\n'",
            "tail -n 20 .docker/research/runtime/dashboard.log 2>/dev/null || true",
        ]
    )
    return {"verification": remote_output(plan, script)}


def deploy(plan: dict[str, Any], *, skip_sync: bool, skip_build: bool) -> dict[str, Any]:
    if plan["deferred"]:
        raise RuntimeError(plan.get("deferred_reason") or "machine is deferred")
    if plan["machine"] != "research":
        raise RuntimeError("non-dry-run deployment is currently enabled only for research")

    remote_run(plan, "command -v docker >/dev/null && docker compose version >/dev/null")
    ensure_research_secrets(plan)
    if not skip_sync:
        sync_repo(plan)
    compose_up(plan, skip_build=skip_build)
    return verify_research(plan)


def main() -> int:
    args = parse_args()
    try:
        inventory = load_inventory(args.inventory)
        plan = machine_plan(inventory, args.machine)
    except (OSError, KeyError, tomllib.TOMLDecodeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2

    if args.dry_run:
        print(json.dumps(plan, indent=2, sort_keys=True))
        return 0

    try:
        result = deploy(plan, skip_sync=args.skip_sync, skip_build=args.skip_build)
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as exc:
        print(str(exc), file=sys.stderr)
        return 2
    print(json.dumps({"plan": plan, "result": result}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
