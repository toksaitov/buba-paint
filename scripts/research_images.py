"""Shared research image metadata and input hashing."""

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
    """Return a stable hash for the files that affect one research image."""
    if image_key not in RESEARCH_IMAGE_INPUTS:
        raise KeyError(f"unknown research image key: {image_key}")
    digest = hashlib.sha256()
    for rel_path in RESEARCH_IMAGE_INPUTS[image_key]:
        add_path_to_hash(digest, repo_root, Path(rel_path))
    return digest.hexdigest()


def all_image_input_hashes(repo_root: Path) -> dict[str, str]:
    """Return input hashes for every research image."""
    return {
        key: image_input_hash(repo_root, key)
        for key in sorted(RESEARCH_IMAGE_INPUTS)
    }


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
