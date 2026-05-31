"""Shared deployment image metadata and input hashing."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path

RESEARCH_IMAGE_INPUTS: dict[str, tuple[str, ...]] = {
    "dashboard": (
        "Cargo.toml",
        "Cargo.lock",
        "crates/buba-machine-telemetry",
        "dashboard/Dockerfile",
        "dashboard/client",
        "dashboard/server",
        "bots/paint/Cargo.toml",
        "agent/Cargo.toml",
    ),
    "research_worker": (
        "Cargo.toml",
        "Cargo.lock",
        "crates/buba-machine-telemetry",
        "dashboard/Dockerfile.research-worker",
        "dashboard/server",
        "bots/paint",
        "agent/Cargo.toml",
    ),
}
LIVE_IMAGE_INPUTS: dict[str, tuple[str, ...]] = {
    "dashboard": RESEARCH_IMAGE_INPUTS["dashboard"],
    "agent": (
        "Cargo.toml",
        "Cargo.lock",
        "crates/buba-machine-telemetry",
        "agent/Dockerfile",
        "agent",
        "bots/paint/Cargo.toml",
        "dashboard/server/Cargo.toml",
    ),
    "paint": (
        "Cargo.toml",
        "Cargo.lock",
        "crates/buba-machine-telemetry",
        "bots/paint/Dockerfile",
        "bots/paint",
        "agent/Cargo.toml",
        "dashboard/server/Cargo.toml",
    ),
    "sidecar": (
        "polymarket-sidecar/Dockerfile",
        "polymarket-sidecar/package.json",
        "polymarket-sidecar/package-lock.json",
        "polymarket-sidecar/src",
        "polymarket-sidecar/tsconfig.json",
    ),
}
IMAGE_INPUTS: dict[str, tuple[str, ...]] = {
    **RESEARCH_IMAGE_INPUTS,
    **LIVE_IMAGE_INPUTS,
}

SKIP_DIRS = {
    ".git",
    ".pytest_cache",
    ".ruff_cache",
    ".vite",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "test-results",
    "__pycache__",
}


def image_input_hash(repo_root: Path, image_key: str) -> str:
    """Return a stable hash for the files that affect one image."""
    if image_key not in IMAGE_INPUTS:
        raise KeyError(f"unknown image key: {image_key}")
    digest = hashlib.sha256()
    for rel_path in IMAGE_INPUTS[image_key]:
        add_path_to_hash(digest, repo_root, Path(rel_path))
    return digest.hexdigest()


def all_image_input_hashes(
    repo_root: Path,
    image_keys: list[str] | tuple[str, ...] | None = None,
) -> dict[str, str]:
    """Return input hashes for the requested images."""
    keys = (
        tuple(image_keys)
        if image_keys is not None
        else tuple(sorted(RESEARCH_IMAGE_INPUTS))
    )
    return {key: image_input_hash(repo_root, key) for key in sorted(keys)}


def add_path_to_hash(digest: "hashlib._Hash", repo_root: Path, rel_path: Path) -> None:
    """Add one file or directory to a content hash."""
    path = repo_root / rel_path
    if path.is_file():
        add_file_to_hash(digest, repo_root, path)
        return
    if not path.is_dir():
        raise FileNotFoundError(path)
    for root, dir_names, file_names in os.walk(path):
        dir_names[:] = sorted(name for name in dir_names if name not in SKIP_DIRS)
        for file_name in sorted(file_names):
            add_file_to_hash(digest, repo_root, Path(root) / file_name)


def add_file_to_hash(digest: "hashlib._Hash", repo_root: Path, path: Path) -> None:
    """Add a file path and content to a content hash."""
    rel = path.relative_to(repo_root).as_posix()
    digest.update(rel.encode("utf-8"))
    digest.update(b"\0")
    digest.update(path.read_bytes())
    digest.update(b"\0")
