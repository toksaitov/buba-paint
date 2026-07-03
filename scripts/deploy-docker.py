#!/usr/bin/env python3
"""Deploy buba-paint to one Docker Compose host."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import secrets
import shlex
import socket
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from research_images import all_image_input_hashes  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SECRET_ENV = REPO_ROOT / ".secrets" / "buba-paint-live-sidecar.env"
DEFAULT_LOCK_FILE = REPO_ROOT / "ops" / "live-images.lock.json"
LIVE_IMAGE_ENVS = {
    "dashboard": "BUBA_DASHBOARD_IMAGE",
    "agent": "BUBA_AGENT_IMAGE",
    "paint": "BUBA_PAINT_IMAGE",
    "sidecar": "BUBA_SIDECAR_IMAGE",
}
LOCK_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")

COMPOSE_FILES = {
    "paper": ["docker-compose.yml", "docker-compose.paper.yml", "docker-compose.prod.yml"],
    "live-readonly": [
        "docker-compose.yml",
        "docker-compose.live-readonly.yml",
        "docker-compose.prod.yml",
    ],
    "live-trading": [
        "docker-compose.yml",
        "docker-compose.live-trading.yml",
        "docker-compose.prod.yml",
    ],
}

SIDECAR_MODES = ("live-readonly", "live-trading")


@dataclass(frozen=True)
class RemoteLayout:
    root: str
    release: str
    runtime: str
    config: str
    caddy_data: str
    caddy_config: str


ENV_KEY_RE = re.compile(r"^[A-Z][A-Z0-9_]*$")
RESERVED_ENV_PREFIXES = ("BUBA_", "COMPOSE_")
RESERVED_ENV_KEYS = {"AGENT_SECRET", "JWT_SECRET", "ADMIN_USER", "ADMIN_PASSWORD"}


def parse_env_set(pairs: list[str]) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for pair in pairs:
        key, sep, value = pair.partition("=")
        key = key.strip()
        if not sep or not key:
            raise ValueError(f"invalid --env-set {pair!r}; expected KEY=VALUE")
        if not ENV_KEY_RE.fullmatch(key):
            raise ValueError(f"invalid --env-set key {key!r}; expected ^[A-Z][A-Z0-9_]*$")
        if key.startswith(RESERVED_ENV_PREFIXES) or key in RESERVED_ENV_KEYS:
            raise ValueError(f"--env-set may not set deploy-reserved key {key!r}")
        if "\n" in value or "\r" in value:
            raise ValueError(f"--env-set value for {key!r} may not contain newlines")
        parsed[key] = value
    return parsed


def load_image_lock(path: Path) -> dict:
    lock = json.loads(path.read_text(encoding="utf-8"))
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
    return lock


def locked_image_env(lock: dict) -> dict[str, str]:
    return {env_name: lock["images"][key]["ref"] for key, env_name in LIVE_IMAGE_ENVS.items()}


def ghcr_pull_token() -> str:
    result = subprocess.run(
        ["gh", "api", "-i", "/user"], cwd=REPO_ROOT, capture_output=True, text=True, check=True
    )
    scopes: set[str] = set()
    for line in result.stdout.splitlines():
        if line.lower().startswith("x-oauth-scopes:"):
            scopes.update(s.strip() for s in line.split(":", 1)[1].split(",") if s.strip())
    if "read:packages" not in scopes and "write:packages" not in scopes:
        raise RuntimeError("gh token lacks read:packages; run `gh auth refresh -s read:packages`")
    return capture(["gh", "auth", "token"]).strip()


def run(cmd: list[str], *, check: bool = True, input_text: str | None = None) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(shlex.quote(part) for part in cmd))
    return subprocess.run(
        cmd,
        check=check,
        input=input_text,
        text=True,
        cwd=REPO_ROOT,
    )


def capture(cmd: list[str], *, check: bool = True) -> str:
    print("+", " ".join(shlex.quote(part) for part in cmd))
    result = subprocess.run(
        cmd,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        cwd=REPO_ROOT,
    )
    return result.stdout


def ssh_hostname(host: str) -> str:
    output = capture(["ssh", "-G", host])
    for line in output.splitlines():
        key, _, value = line.partition(" ")
        if key == "hostname" and value:
            return value.strip()
    return host


def resolve_ips(name: str) -> set[str]:
    try:
        return {
            item[4][0]
            for item in socket.getaddrinfo(name, None, family=socket.AF_INET, type=socket.SOCK_STREAM)
        }
    except socket.gaierror:
        return set()


def verify_domain_points_to_host(host: str, domain: str) -> None:
    target = ssh_hostname(host)
    host_ips = resolve_ips(target)
    domain_ips = resolve_ips(domain)
    if not host_ips:
        raise RuntimeError(f"could not resolve SSH host target {target!r}")
    if not domain_ips:
        raise RuntimeError(f"could not resolve deployment domain {domain!r}")
    if host_ips.isdisjoint(domain_ips):
        raise RuntimeError(
            "deployment domain does not point at the SSH host; "
            f"{domain} -> {sorted(domain_ips)}, {host} ({target}) -> {sorted(host_ips)}. "
            f"Update the DNS A record for {domain} to one of {sorted(host_ips)} before running Caddy TLS deployment."
        )


def ssh(host: str, remote_cmd: str, *, check: bool = True) -> subprocess.CompletedProcess[str]:
    return run(["ssh", host, remote_cmd], check=check)


def ssh_capture(host: str, remote_cmd: str, *, check: bool = True) -> str:
    return capture(["ssh", host, remote_cmd], check=check)


def scp(local: Path, host: str, remote: str) -> None:
    run(["scp", str(local), f"{host}:{remote}"])


def install_docker(host: str) -> None:
    ssh(
        host,
        r"""
set -euo pipefail
if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
  docker --version
  docker compose version
  exit 0
fi
sudo apt-get update
sudo apt-get install -y ca-certificates curl gnupg
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg
. /etc/os-release
codename="${VERSION_CODENAME:-${UBUNTU_CODENAME:-}}"
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu ${codename} stable" \
  | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
if ! sudo apt-get update; then
  echo "Docker apt repo did not accept Ubuntu codename ${codename}; falling back to noble" >&2
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu noble stable" \
    | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
  sudo apt-get update
fi
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo usermod -aG docker "$USER" || true
docker --version
docker compose version
""",
    )


def ensure_swap(host: str, size_gb: int) -> None:
    if size_gb <= 0:
        return
    ssh(
        host,
        f"""
set -euo pipefail
if swapon --show=NAME --noheadings | grep -q .; then
  swapon --show
  exit 0
fi
free_mb=$(df --output=avail -m / | tail -n 1 | tr -d ' ')
needed_mb=$(({size_gb} * 1024 + 512))
if [ "$free_mb" -lt "$needed_mb" ]; then
  echo "not enough free disk for {size_gb}G swap file: ${{free_mb}} MiB available" >&2
  exit 1
fi
sudo fallocate -l {size_gb}G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile >/dev/null
sudo swapon /swapfile
if ! grep -q '^/swapfile ' /etc/fstab; then
  echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab >/dev/null
fi
swapon --show
""",
    )


def remote_home(host: str) -> str:
    return ssh_capture(host, "printf %s \"$HOME\"").strip()


def remote_ids(host: str) -> tuple[str, str]:
    uid = ssh_capture(host, "id -u").strip()
    gid = ssh_capture(host, "id -g").strip()
    return uid, gid


def make_layout(root: str, stamp: str, mode: str, runtime_name: str | None) -> RemoteLayout:
    runtime = runtime_name or f"docker-{mode}-{stamp}"
    return RemoteLayout(
        root=root,
        release=f"{root}/releases/{stamp}",
        runtime=f"{root}/runtime/{runtime}",
        config=f"{root}/config",
        caddy_data=f"{root}/caddy/data",
        caddy_config=f"{root}/caddy/config",
    )


def write_secret_files(
    tmp: Path,
    *,
    layout: RemoteLayout,
    domain: str,
    email: str,
    mode: str,
    uid: str,
    gid: str,
    sidecar_env: Path,
    balance: str,
    extra_env: dict[str, str],
    locked_images: dict[str, str] | None = None,
) -> dict[str, str]:
    agent_secret = secrets.token_urlsafe(48)
    jwt_secret = secrets.token_urlsafe(64)
    admin_user = "admin"
    admin_password = secrets.token_urlsafe(32)
    dashboard_toml = f"""[server]
port = 3001
jwt_secret = {json.dumps(jwt_secret)}

[[agents]]
id = "paint"
name = "BTC Paint Docker ({mode})"
url = "http://agent:9090"
secret = {json.dumps(agent_secret)}
"""
    caddyfile = f"""{{
  email {email}
}}

{domain} {{
  encode zstd gzip
  reverse_proxy dashboard:3001
}}
"""
    env = {
        "COMPOSE_PROJECT_NAME": "buba-paint",
        "BUBA_DOMAIN": domain,
        "BUBA_RUNTIME_DIR": layout.runtime,
        "BUBA_DASHBOARD_CONFIG": f"{layout.config}/dashboard.toml",
        "BUBA_SIDECAR_ENV": f"{layout.config}/sidecar.env",
        "BUBA_CADDYFILE": f"{layout.config}/Caddyfile",
        "BUBA_CADDY_DATA_DIR": layout.caddy_data,
        "BUBA_CADDY_CONFIG_DIR": layout.caddy_config,
        "BUBA_UID": uid,
        "BUBA_GID": gid,
        "BUBA_PAINT_BALANCE": balance,
        "AGENT_SECRET": agent_secret,
        "ADMIN_USER": admin_user,
        "ADMIN_PASSWORD": admin_password,
        "JWT_SECRET": jwt_secret,
        "LOG_LEVEL": "info",
        "RUST_LOG": "info",
    }
    managed_collisions = sorted(set(extra_env) & set(env))
    if managed_collisions:
        raise ValueError(
            f"--env-set may not override deploy-managed keys: {', '.join(managed_collisions)}"
        )
    if mode == "live-trading":
        env["LIVE_DRY_RUN"] = "true"
    if locked_images:
        env.update(locked_images)
    env.update(extra_env)
    (tmp / ".env").write_text(
        "\n".join(f"{key}={value}" for key, value in env.items()) + "\n",
        encoding="utf-8",
    )
    (tmp / "dashboard.toml").write_text(dashboard_toml, encoding="utf-8")
    (tmp / "Caddyfile").write_text(caddyfile, encoding="utf-8")
    (tmp / "dashboard-admin-password.txt").write_text(admin_password + "\n", encoding="utf-8")
    sidecar_text = sidecar_env.read_text(encoding="utf-8")
    (tmp / "sidecar.env").write_text(sidecar_text, encoding="utf-8")
    return {
        "admin_user": admin_user,
        "dashboard_password_remote_path": f"{layout.runtime}/dashboard-admin-password.txt",
        "mode": mode,
    }


def stage_release(host: str, layout: RemoteLayout) -> None:
    ssh(
        host,
        f"set -euo pipefail; mkdir -p {shlex.quote(layout.release)} {shlex.quote(layout.runtime)} {shlex.quote(layout.config)} {shlex.quote(layout.caddy_data)} {shlex.quote(layout.caddy_config)}",
    )
    run(
        [
            "rsync",
            "-az",
            "--delete",
            "--exclude=.git/",
            "--exclude=.secrets/",
            "--exclude=.docker/",
            "--exclude=target/",
            "--exclude=data/",
            "--exclude=runs/",
            "--exclude=dashboard/client/node_modules/",
            "--exclude=dashboard/client/dist/",
            "--exclude=dashboard/client/coverage/",
            "--exclude=dashboard/client/test-results/",
            "--exclude=dashboard/client/playwright-report/",
            "--exclude=polymarket-sidecar/node_modules/",
            "--exclude=polymarket-sidecar/dist/",
            "--exclude=__pycache__/",
            "--exclude=*.pyc",
            "--exclude=*.db",
            "--exclude=*.db-wal",
            "--exclude=*.db-shm",
            "./",
            f"{host}:{layout.release}/",
        ]
    )
    ssh(host, f"ln -sfn {shlex.quote(layout.release)} {shlex.quote(layout.root + '/current')}")


def upload_runtime_config(host: str, layout: RemoteLayout, tmp: Path) -> None:
    for name, remote in [
        (".env", f"{layout.release}/.env"),
        ("dashboard.toml", f"{layout.config}/dashboard.toml"),
        ("Caddyfile", f"{layout.config}/Caddyfile"),
        ("sidecar.env", f"{layout.config}/sidecar.env"),
        ("dashboard-admin-password.txt", f"{layout.runtime}/dashboard-admin-password.txt"),
    ]:
        scp(tmp / name, host, remote)
    ssh(
        host,
        "set -euo pipefail; "
        f"chmod 600 {shlex.quote(layout.release + '/.env')} "
        f"{shlex.quote(layout.config + '/dashboard.toml')} "
        f"{shlex.quote(layout.config + '/sidecar.env')} "
        f"{shlex.quote(layout.runtime + '/dashboard-admin-password.txt')}",
    )


def compose_files(mode: str, *, use_locked: bool = False, parked: bool = False) -> str:
    files = list(COMPOSE_FILES[mode])
    if use_locked:
        files.append("docker-compose.live-stopped.yml")
    if parked:
        files.append("docker-compose.parked.yml")
    return " ".join(f"-f {shlex.quote(name)}" for name in files)


def build_services(mode: str) -> str:
    services = ["dashboard", "agent", "paint"]
    if mode in SIDECAR_MODES:
        services.insert(0, "sidecar")
    return " ".join(shlex.quote(service) for service in services)


def stop_old_processes(
    host: str, layout: RemoteLayout, mode: str, *, use_locked: bool = False, parked: bool = False
) -> None:
    files = compose_files(mode, use_locked=use_locked, parked=parked)
    ssh(
        host,
        f"""
set -euo pipefail
systemctl --user stop buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
systemctl --user disable buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
if command -v docker >/dev/null 2>&1; then
  cd {shlex.quote(layout.release)}
  sudo docker compose --env-file .env {files} down --remove-orphans || true
fi
""",
    )


def start_stack(
    host: str,
    layout: RemoteLayout,
    mode: str,
    *,
    lock: dict | None = None,
    token: str | None = None,
    parked: bool = False,
) -> None:
    files = compose_files(mode, use_locked=lock is not None, parked=parked)
    services = build_services(mode)
    if lock is None:
        ssh(
            host,
            f"""
set -euo pipefail
cd {shlex.quote(layout.release)}
base="sudo env COMPOSE_PARALLEL_LIMIT=1 docker compose --env-file .env {files}"
for service in {services}; do
  $base build "$service"
done
$base up -d --no-build
""",
        )
        return
    token_b64 = base64.b64encode((token or "").encode("utf-8")).decode("ascii")
    namespace = lock.get("namespace") or "toksaitov"
    ssh(
        host,
        f"""
set -euo pipefail
cd {shlex.quote(layout.release)}
docker_config=$(mktemp -d /tmp/buba-live-ghcr.XXXXXX)
cleanup() {{ sudo env DOCKER_CONFIG="$docker_config" docker logout ghcr.io >/dev/null 2>&1 || true; rm -rf "$docker_config" >/dev/null 2>&1 || true; }}
trap cleanup EXIT
printf '%s' {shlex.quote(token_b64)} | base64 -d \
  | sudo env DOCKER_CONFIG="$docker_config" docker login ghcr.io -u {shlex.quote(namespace)} --password-stdin >/dev/null
base="sudo env COMPOSE_PARALLEL_LIMIT=1 DOCKER_CONFIG=$docker_config docker compose --env-file .env {files}"
$base config --quiet
$base pull {services}
$base up -d --no-build
""",
    )


def remote_compose(
    host: str,
    layout: RemoteLayout,
    mode: str,
    args: str,
    *,
    check: bool = True,
    use_locked: bool = False,
    parked: bool = False,
) -> str:
    files = compose_files(mode, use_locked=use_locked, parked=parked)
    return ssh_capture(
        host,
        f"set -euo pipefail; cd {shlex.quote(layout.release)}; sudo docker compose --env-file .env {files} {args}",
        check=check,
    )


def verify(
    host: str, layout: RemoteLayout, mode: str, domain: str, *, use_locked: bool = False, parked: bool = False
) -> dict[str, object]:
    def rc(args: str, *, check: bool = True) -> str:
        return remote_compose(host, layout, mode, args, check=check, use_locked=use_locked, parked=parked)

    checks: dict[str, object] = {}
    checks["compose_ps"] = rc("ps")
    checks["sidecar_health"] = rc(
        "exec -T sidecar curl -fsS http://localhost:3210/health",
        check=mode in SIDECAR_MODES,
    )
    checks["agent_health"] = rc("exec -T agent curl -fsS http://localhost:9090/health")
    checks["dashboard_health"] = rc("exec -T dashboard curl -fsS http://localhost:3001/health")
    checks["db_quick_check"] = rc(
        "exec -T paint sqlite3 /runtime/paint.db 'PRAGMA quick_check;'"
    ).strip()
    checks["public_http"] = capture(["curl", "-I", "--max-time", "20", f"http://{domain}"], check=False)
    checks["public_https"] = capture(["curl", "-fsS", "--max-time", "20", f"https://{domain}/health"], check=False)
    checks["certificate"] = capture(
        [
            "bash",
            "-lc",
            f"echo | openssl s_client -connect {shlex.quote(domain)}:443 -servername {shlex.quote(domain)} 2>/dev/null | openssl x509 -noout -subject -issuer -dates",
        ],
        check=False,
    )
    checks["live_table_counts"] = rc(
        "exec -T paint sqlite3 /runtime/paint.db \"SELECT 'live_order_intents', count(*) FROM live_order_intents UNION ALL SELECT 'live_orders', count(*) FROM live_orders UNION ALL SELECT 'live_redemptions', count(*) FROM live_redemptions UNION ALL SELECT 'live_control_commands', count(*) FROM live_control_commands;\"",
        check=False,
    )
    checks["log_tail"] = ssh_capture(
        host,
        f"set -euo pipefail; for name in sidecar paint agent dashboard; do echo --- $name; tail -n 40 {shlex.quote(layout.runtime)}/$name.log 2>/dev/null || true; done",
        check=False,
    )
    return checks


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="buba-paint")
    parser.add_argument("--domain", default="buba.toksaitov.com")
    parser.add_argument("--mode", choices=sorted(COMPOSE_FILES), default="live-readonly")
    parser.add_argument("--email", default="admin@toksaitov.com")
    parser.add_argument("--remote-root", default="~/buba-paint-live")
    parser.add_argument("--runtime-name")
    parser.add_argument("--balance", default="100")
    parser.add_argument("--sidecar-env", type=Path, default=DEFAULT_SECRET_ENV)
    parser.add_argument(
        "--env-set",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="inject KEY=VALUE into the generated .env (compose interpolation); repeatable",
    )
    parser.add_argument(
        "--use-locked-images",
        action="store_true",
        help="pull digest-pinned images from the lock instead of building on the host",
    )
    parser.add_argument("--lock-file", type=Path, default=DEFAULT_LOCK_FILE)
    parser.add_argument(
        "--parked",
        action="store_true",
        help="battle-mode staging: bring the stack up but init the DB and sleep the paint bot "
        "(no live trading, no feed capture, no DB growth); redeploy without --parked to go live",
    )
    parser.add_argument("--install-docker", action="store_true")
    parser.add_argument("--ensure-swap-gb", type=int, default=4)
    parser.add_argument("--skip-dns-check", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    stamp = datetime.now(UTC).strftime("%Y%m%d-%H%M%S")
    try:
        extra_env = parse_env_set(args.env_set)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    if args.dry_run:
        live_dry_run = extra_env.get(
            "LIVE_DRY_RUN", "true (explicit .env seed)" if args.mode == "live-trading" else "n/a"
        )
        lock_status: dict | str
        if args.use_locked_images:
            try:
                lk = load_image_lock(args.lock_file)
                lock_status = {"ok": True, "commit": lk.get("commit"), "tag": lk.get("tag")}
            except (OSError, ValueError, json.JSONDecodeError) as exc:
                lock_status = {"ok": False, "error": str(exc)}
        else:
            lock_status = "not used (host build)"
        files = list(COMPOSE_FILES[args.mode])
        if args.use_locked_images:
            files.append("docker-compose.live-stopped.yml")
        if args.parked:
            files.append("docker-compose.parked.yml")
        print(
            json.dumps(
                {
                    "host": args.host,
                    "domain": args.domain,
                    "mode": args.mode,
                    "install_docker": args.install_docker,
                    "ensure_swap_gb": args.ensure_swap_gb,
                    "compose_files": files,
                    "use_locked_images": args.use_locked_images,
                    "parked": args.parked,
                    "lock": lock_status,
                    "sidecar_env_exists": args.sidecar_env.exists(),
                    "env_set": extra_env,
                    "live_dry_run_effective": live_dry_run,
                },
                indent=2,
            )
        )
        return 0

    if args.mode in SIDECAR_MODES and not args.sidecar_env.is_file():
        print(f"missing sidecar env: {args.sidecar_env}", file=sys.stderr)
        return 2

    lock: dict | None = None
    token: str | None = None
    locked_images: dict[str, str] | None = None
    if args.use_locked_images:
        try:
            lock = load_image_lock(args.lock_file)
            token = ghcr_pull_token()
        except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as exc:
            print(f"locked-image deploy unavailable: {exc}", file=sys.stderr)
            return 2
        locked_images = locked_image_env(lock)

    if not args.skip_dns_check:
        try:
            verify_domain_points_to_host(args.host, args.domain)
        except RuntimeError as exc:
            print(str(exc), file=sys.stderr)
            return 2

    if args.install_docker:
        install_docker(args.host)
    ensure_swap(args.host, args.ensure_swap_gb)

    home = remote_home(args.host)
    root = args.remote_root.replace("~", home)
    uid, gid = remote_ids(args.host)
    layout = make_layout(root, stamp, args.mode, args.runtime_name)

    stage_release(args.host, layout)
    with tempfile.TemporaryDirectory(prefix="buba-docker-deploy-") as tmp_raw:
        tmp = Path(tmp_raw)
        secret_report = write_secret_files(
            tmp,
            layout=layout,
            domain=args.domain,
            email=args.email,
            mode=args.mode,
            uid=uid,
            gid=gid,
            sidecar_env=args.sidecar_env,
            balance=args.balance,
            extra_env=extra_env,
            locked_images=locked_images,
        )
        upload_runtime_config(args.host, layout, tmp)

    use_locked = lock is not None
    stop_old_processes(args.host, layout, args.mode, use_locked=use_locked, parked=args.parked)
    start_stack(args.host, layout, args.mode, lock=lock, token=token, parked=args.parked)
    checks = verify(args.host, layout, args.mode, args.domain, use_locked=use_locked, parked=args.parked)

    evidence_dir = REPO_ROOT / "data" / "experiments" / f"docker-deploy-{stamp}"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    (evidence_dir / "notes.md").write_text(
        "\n".join(
            [
                "# Docker Deployment Evidence",
                "",
                f"- Host: `{args.host}`",
                f"- Domain: `{args.domain}`",
                f"- Mode: `{args.mode}`",
                f"- Release: `{layout.release}`",
                f"- Runtime: `{layout.runtime}`",
                f"- Dashboard user: `{secret_report['admin_user']}`",
                f"- Dashboard password on host: `{secret_report['dashboard_password_remote_path']}`",
                "",
                "Secrets are not copied into this evidence directory.",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    (evidence_dir / "manifest.json").write_text(
        json.dumps(
            {
                "host": args.host,
                "domain": args.domain,
                "mode": args.mode,
                "release": layout.release,
                "runtime": layout.runtime,
                "config": layout.config,
                "caddy_data": layout.caddy_data,
                "dashboard_user": secret_report["admin_user"],
                "dashboard_password_remote_path": secret_report["dashboard_password_remote_path"],
                "checks": checks,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"Evidence: {evidence_dir}")
    print(f"Dashboard password: ssh {args.host} 'cat {shlex.quote(secret_report['dashboard_password_remote_path'])}'")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
