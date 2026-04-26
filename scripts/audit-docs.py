#!/usr/bin/env python3
"""Audit documentation and helper hygiene for repo-local drift."""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SKIP_DIR_NAMES = {
    ".git",
    ".entire",
    "target",
    "node_modules",
    "dist",
    "coverage",
    "test-results",
    "playwright-report",
}

DOC_MARKERS = {
    "readme.md",
    "notes.md",
    "postmortem.md",
    "sweep_blocked.md",
    "running.md",
}

STALE_REFERENCES = {
    "dashboard/client/README.md": "Use dashboard/client/Readme.md.",
    "pages/trading.tsx": "Execution aliases are route redirects, not page files.",
    "pages/live.tsx": "Execution aliases are route redirects, not page files.",
    "hooks/use-bot-status.ts": "This hook has been removed.",
    "hooks/use-balance.ts": "Use equity-series and status hooks instead.",
    "components/dashboard/stat-card.tsx": "This component has been removed.",
    "scripts/run-006": "Run-specific scripts now live under scripts/archive/run-006.",
    "legacy-ts": "The legacy TypeScript implementation was removed from the active tree.",
}

LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")


def should_skip(path: Path) -> bool:
    """Return true when a path belongs to generated or vendor content."""
    return any(part in SKIP_DIR_NAMES for part in path.parts)


def repo_files() -> list[Path]:
    """Return source-controlled and local text candidates under the repo."""
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if should_skip(path) or not path.is_file():
            continue
        files.append(path)
    return files


def display(path: Path) -> str:
    """Format a path relative to the repository root."""
    return str(path.relative_to(ROOT))


def audit_readme_case(errors: list[str]) -> None:
    """Reject repo-owned uppercase readme files."""
    for path in repo_files():
        if path.name == "README.md":
            errors.append(f"{display(path)} should be named Readme.md")


def audit_forbidden_root_files(errors: list[str]) -> None:
    """Reject known scratch files from the repo root."""
    for name in ["PLAN.md"]:
        if (ROOT / name).exists():
            errors.append(f"{name} should not exist at repository root")


def normalize_link_target(source: Path, raw: str) -> Path | None:
    """Resolve a Markdown link target if it is local to this repository."""
    target = raw.strip()
    if not target or target.startswith(("#", "http://", "https://", "mailto:", "app://")):
        return None
    target = target.split("#", 1)[0].strip()
    if not target:
        return None
    if " " in target and not target.startswith("<"):
        target = target.split()[0]
    target = target.strip("<>")
    if not target:
        return None
    candidate = Path(target) if target.startswith("/") else source.parent / target
    try:
        resolved = candidate.resolve().relative_to(ROOT)
    except ValueError:
        return None
    return ROOT / resolved


def audit_markdown_links(errors: list[str]) -> None:
    """Reject broken local Markdown links outside ignored tool caches."""
    for path in repo_files():
        if path.suffix.lower() != ".md":
            continue
        text = path.read_text(errors="ignore")
        for match in LINK_RE.finditer(text):
            target = normalize_link_target(path, match.group(1))
            if target is None or target.exists():
                continue
            line = text[: match.start()].count("\n") + 1
            errors.append(
                f"{display(path)}:{line} broken local link {match.group(1)} -> {display(target)}"
            )


def audit_data_docs(errors: list[str]) -> None:
    """Require local context docs for every data subdirectory."""
    data_root = ROOT / "data"
    if not data_root.exists():
        return
    for path in sorted(p for p in data_root.rglob("*") if p.is_dir()):
        names = {child.name.lower() for child in path.iterdir() if child.is_file()}
        if DOC_MARKERS.isdisjoint(names):
            errors.append(f"{display(path)} needs Readme.md, notes.md, or equivalent context")


def looks_text(path: Path) -> bool:
    """Return true for text-like files worth scanning for stale references."""
    if path.name == "audit-docs.py":
        return False
    return path.suffix.lower() in {
        ".md",
        ".py",
        ".sh",
        ".mjs",
        ".js",
        ".ts",
        ".tsx",
        ".toml",
        ".yml",
        ".yaml",
        ".json",
        ".sql",
        ".example",
    } or path.name in {"Makefile", ".env.example", ".gitignore", ".gitattributes", ".dockerignore"}


def audit_stale_references(errors: list[str]) -> None:
    """Reject references to removed files and obsolete layout names."""
    for path in repo_files():
        if not looks_text(path):
            continue
        text = path.read_text(errors="ignore")
        for needle, reason in STALE_REFERENCES.items():
            if needle in text:
                errors.append(f"{display(path)} references {needle}: {reason}")


def audit_data_wal_shm(errors: list[str]) -> None:
    """Reject transient SQLite sidecar files under derived data."""
    data_root = ROOT / "data"
    if not data_root.exists():
        return
    for path in data_root.rglob("*"):
        if path.suffix in {".db-wal", ".db-shm"}:
            errors.append(f"{display(path)} is transient derived SQLite state")


def main() -> int:
    """Run all hygiene checks and return a shell status code."""
    os.chdir(ROOT)
    errors: list[str] = []
    audit_readme_case(errors)
    audit_forbidden_root_files(errors)
    audit_markdown_links(errors)
    audit_data_docs(errors)
    audit_stale_references(errors)
    audit_data_wal_shm(errors)
    if errors:
        print("docs audit failed:")
        for error in errors:
            print(f"- {error}")
        return 1
    print("docs audit passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
