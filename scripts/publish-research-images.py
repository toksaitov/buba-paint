#!/usr/bin/env python3
"""Build and publish digest-locked research images to GHCR."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

from research_images import all_image_input_hashes

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK_FILE = REPO_ROOT / "ops" / "research-images.lock.json"
DEFAULT_SOURCE = "https://github.com/toksaitov/buba-paint"
TARGET_PLATFORM = "linux/amd64"
IMAGE_DEFINITIONS = {
    "dashboard": {
        "repository": "ghcr.io/toksaitov/buba-paint-dashboard",
        "dockerfile": "dashboard/Dockerfile",
        "title": "Buba Paint research dashboard",
        "description": "Dashboard and static frontend for Buba research orchestration.",
    },
    "research_worker": {
        "repository": "ghcr.io/toksaitov/buba-paint-research-worker",
        "dockerfile": "dashboard/Dockerfile.research-worker",
        "title": "Buba Paint research worker",
        "description": "Research worker with backtest, transfer, and telemetry tooling.",
    },
}


def parse_args() -> argparse.Namespace:
    """Parse CLI arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--tag")
    parser.add_argument("--lock-file", type=Path, default=DEFAULT_LOCK_FILE)
    parser.add_argument("--source", default=DEFAULT_SOURCE)
    parser.add_argument("--namespace", default="toksaitov")
    return parser.parse_args()


def run(
    cmd: list[str],
    *,
    env: dict[str, str] | None = None,
    input_text: str | None = None,
    capture: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run a command and fail on non-zero exit."""
    return subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        env=env,
        input=input_text,
        text=True,
        check=True,
        capture_output=capture,
    )


def output(cmd: list[str], *, env: dict[str, str] | None = None) -> str:
    """Run a command and return stdout."""
    return run(cmd, env=env, capture=True).stdout.strip()


def git_commit() -> str:
    """Return the current Git commit SHA."""
    return output(["git", "rev-parse", "HEAD"])


def default_tag(commit: str) -> str:
    """Return the default publish tag."""
    stamp = dt.datetime.now(dt.UTC).strftime("%Y%m%d-%H%M%S")
    return f"research-{stamp}-{commit[:12]}"


def gh_scopes() -> set[str]:
    """Return scopes for the current gh token."""
    try:
        result = run(["gh", "api", "-i", "/user"], capture=True)
    except subprocess.CalledProcessError:
        return set()
    scopes: set[str] = set()
    for line in result.stdout.splitlines():
        if line.lower().startswith("x-oauth-scopes:"):
            value = line.split(":", 1)[1]
            scopes.update(scope.strip() for scope in value.split(",") if scope.strip())
    return scopes


def gh_token() -> str:
    """Return the current gh auth token."""
    return output(["gh", "auth", "token"])


def require_publish_auth(scopes: set[str]) -> None:
    """Fail when the current gh token cannot publish GHCR images."""
    if "write:packages" not in scopes:
        raise RuntimeError(
            "gh token lacks write:packages; run `gh auth refresh -s write:packages`"
        )


def image_definitions(namespace: str) -> dict[str, dict[str, str]]:
    """Return image definitions for the requested namespace."""
    prefix = f"ghcr.io/{namespace}/"
    return {
        key: {**value, "repository": f"{prefix}{value['repository'].rsplit('/', 1)[1]}"}
        for key, value in IMAGE_DEFINITIONS.items()
    }


def build_and_push(
    key: str,
    definition: dict[str, str],
    *,
    tag: str,
    commit: str,
    source: str,
    created_at: str,
    env: dict[str, str],
) -> dict[str, str]:
    """Build, push, and return lock metadata for one image."""
    image = f"{definition['repository']}:{tag}"
    labels = {
        "org.opencontainers.image.source": source,
        "org.opencontainers.image.revision": commit,
        "org.opencontainers.image.created": created_at,
        "org.opencontainers.image.title": definition["title"],
        "org.opencontainers.image.description": definition["description"],
    }
    with tempfile.TemporaryDirectory(prefix=f"buba-{key}-metadata-") as tmp_dir:
        metadata_file = Path(tmp_dir) / "metadata.json"
        build_cmd = [
            "docker",
            "buildx",
            "build",
            "--platform",
            TARGET_PLATFORM,
            "--file",
            definition["dockerfile"],
            "--tag",
            image,
            "--push",
            "--metadata-file",
            str(metadata_file),
        ]
        for label_key, label_value in labels.items():
            build_cmd.extend(["--label", f"{label_key}={label_value}"])
        build_cmd.append(".")
        run(build_cmd, env=env)
        digest = read_metadata_digest(metadata_file)
    return {
        "repository": definition["repository"],
        "tag": tag,
        "ref": f"{definition['repository']}@{digest}",
        "digest": digest,
        "dockerfile": definition["dockerfile"],
        "platform": TARGET_PLATFORM,
        "title": definition["title"],
        "description": definition["description"],
        "key": key,
    }


def read_metadata_digest(path: Path) -> str:
    """Read the pushed image digest from a Buildx metadata file."""
    metadata = json.loads(path.read_text(encoding="utf-8"))
    digest = metadata.get("containerimage.digest")
    if not isinstance(digest, str) or not digest.startswith("sha256:"):
        raise RuntimeError(f"Buildx metadata did not contain an image digest: {path}")
    return digest


def write_lock(path: Path, lock: dict[str, Any]) -> None:
    """Write the research image lock file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def dry_run_payload(args: argparse.Namespace, commit: str, tag: str, scopes: set[str]) -> dict[str, Any]:
    """Return the dry-run plan."""
    definitions = image_definitions(args.namespace)
    return {
        "commit": commit,
        "images": {
            key: {
                "repository": definition["repository"],
                "dockerfile": definition["dockerfile"],
                "platform": TARGET_PLATFORM,
                "tag": tag,
            }
            for key, definition in definitions.items()
        },
        "input_hashes": all_image_input_hashes(REPO_ROOT),
        "lock_file": str(args.lock_file),
        "source": args.source,
        "write_packages": "write:packages" in scopes,
        "auth_hint": None
        if "write:packages" in scopes
        else "run `gh auth refresh -s write:packages` before publishing",
    }


def main() -> int:
    """Run the image publisher."""
    args = parse_args()
    try:
        commit = git_commit()
        tag = args.tag or default_tag(commit)
        scopes = gh_scopes()
        if args.dry_run:
            print(json.dumps(dry_run_payload(args, commit, tag, scopes), indent=2, sort_keys=True))
            return 0
        require_publish_auth(scopes)
        token = gh_token()
        created_at = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat()
        definitions = image_definitions(args.namespace)
        env = dict(os.environ)
        run(["docker", "login", "ghcr.io", "-u", args.namespace, "--password-stdin"], env=env, input_text=token)
        images = {
            key: build_and_push(
                key,
                definition,
                tag=tag,
                commit=commit,
                source=args.source,
                created_at=created_at,
                env=env,
            )
            for key, definition in definitions.items()
        }
        lock = {
            "schema": 1,
            "created_at": created_at,
            "source": args.source,
            "commit": commit,
            "tag": tag,
            "registry": "ghcr.io",
            "namespace": args.namespace,
            "input_hashes": all_image_input_hashes(REPO_ROOT),
            "images": images,
        }
        write_lock(args.lock_file, lock)
        print(json.dumps(lock, indent=2, sort_keys=True))
        return 0
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError) as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
