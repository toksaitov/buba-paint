#!/usr/bin/env python3
"""Reject non-ASCII dash punctuation in user-facing source text."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SCAN_ROOTS = (
    "agent/src",
    "bots/paint/src",
    "bots/paint/tests",
    "dashboard/client/src",
    "dashboard/server/src",
    "polymarket-sidecar/src",
    "scripts",
)

TARGET_SUFFIXES = {".js", ".mjs", ".py", ".rs", ".ts", ".tsx"}
FORBIDDEN = {
    "\u2013": "en dash",
    "\u2014": "em dash",
}


def tracked_files() -> list[Path]:
    """Return tracked files under user-facing source roots."""
    result = subprocess.run(
        ["git", "ls-files", *SCAN_ROOTS],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return [
        ROOT / line
        for line in result.stdout.splitlines()
        if line and Path(line).suffix in TARGET_SUFFIXES
    ]


def scan_file(path: Path) -> list[str]:
    """Return style violations for one file."""
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return []
    errors: list[str] = []
    rel = path.relative_to(ROOT)
    for line_no, line in enumerate(text.splitlines(), start=1):
        for char, name in FORBIDDEN.items():
            if char in line:
                errors.append(f"{rel}:{line_no}: replace {name} with ASCII punctuation")
    return errors


def main() -> int:
    """Run the audit."""
    errors: list[str] = []
    for path in tracked_files():
        errors.extend(scan_file(path))
    if errors:
        print("user-facing text audit failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print("user-facing text audit passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
