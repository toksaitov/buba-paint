#!/usr/bin/env python3
"""Run component coverage checks and enforce minimum regression floors."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLIENT_DIR = ROOT / "dashboard" / "client"

RUST_TARGETS = {
    "buba-paint": {
        "cmd": [
            "cargo",
            "llvm-cov",
            "--summary-only",
            "-p",
            "buba-paint",
            "--lib",
            "--tests",
            "--ignore-filename-regex",
            r"main\.rs$",
        ],
        "minimum": 80.0,
    },
    "buba-agent": {
        "cmd": [
            "cargo",
            "llvm-cov",
            "--summary-only",
            "-p",
            "buba-agent",
            "--lib",
            "--tests",
            "--ignore-filename-regex",
            r"main\.rs$",
        ],
        "minimum": 90.0,
    },
    "buba-dashboard": {
        "cmd": [
            "cargo",
            "llvm-cov",
            "--summary-only",
            "-p",
            "buba-dashboard",
            "--lib",
            "--tests",
            "--ignore-filename-regex",
            r"main\.rs$",
        ],
        "minimum": 90.0,
    },
}

FRONTEND_MINIMUM = 80.0
FRONTEND_RESEARCH_MINIMUM = 70.0
RESEARCH_RUST_MINIMUM = 80.0
RESEARCH_RUST_SCOPE_RE = re.compile(
    r"(?:^|/)(?:research_[^/]*\.rs|api/research[^/]*\.rs|bin/buba-research-worker\.rs)$"
)
FRONTEND_RESEARCH_SCOPE_RE = re.compile(r"client/src/.*research")
ANSI_ESCAPE_RE = re.compile(r"\x1b\[[0-9;]*m")


def run(cmd: list[str], cwd: Path = ROOT) -> str:
    result = subprocess.run(
        cmd,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )
    output = result.stdout + result.stderr
    sys.stdout.write(output)
    if result.returncode != 0:
        raise SystemExit(result.returncode)
    return output


def parse_rust_line_coverage(output: str) -> float:
    for raw_line in output.splitlines():
        line = ANSI_ESCAPE_RE.sub("", raw_line).strip()
        if not line.startswith("TOTAL"):
            continue
        percentages = re.findall(r"(\d+(?:\.\d+)?)%", line)
        if len(percentages) < 3:
            raise SystemExit("failed to parse Rust TOTAL line coverage columns")
        return float(percentages[2])
    raise SystemExit("failed to parse Rust coverage summary")


def parse_frontend_line_coverage() -> float:
    summary_path = CLIENT_DIR / "coverage" / "coverage-summary.json"
    if not summary_path.exists():
        raise SystemExit("frontend coverage summary was not generated")
    data = json.loads(summary_path.read_text())
    return float(data["total"]["lines"]["pct"])


def parse_research_rust_line_coverage() -> float:
    """Aggregate llvm-cov JSON line coverage over research-scoped server files."""
    summary_path = ROOT / "target" / "llvm-cov" / "research-summary.json"
    if not summary_path.exists():
        raise SystemExit("research Rust coverage summary was not generated")
    data = json.loads(summary_path.read_text())
    covered = 0
    total = 0
    for export in data.get("data", []):
        for file_entry in export.get("files", []):
            filename = file_entry.get("filename", "").replace("\\", "/")
            if not RESEARCH_RUST_SCOPE_RE.search(filename):
                continue
            lines = file_entry["summary"]["lines"]
            covered += int(lines["covered"])
            total += int(lines["count"])
    if total == 0:
        raise SystemExit("research Rust coverage matched no files")
    return 100.0 * covered / total


def parse_research_frontend_line_coverage() -> float:
    """Aggregate vitest line coverage over research-scoped client files."""
    summary_path = CLIENT_DIR / "coverage" / "coverage-summary.json"
    if not summary_path.exists():
        raise SystemExit("frontend coverage summary was not generated")
    data = json.loads(summary_path.read_text())
    covered = 0
    total = 0
    for key, entry in data.items():
        if key == "total":
            continue
        if not FRONTEND_RESEARCH_SCOPE_RE.search(key.replace("\\", "/")):
            continue
        lines = entry["lines"]
        covered += int(lines["covered"])
        total += int(lines["total"])
    if total == 0:
        raise SystemExit("research frontend coverage matched no files")
    return 100.0 * covered / total


def main() -> int:
    failures: list[str] = []
    for name, target in RUST_TARGETS.items():
        output = run(target["cmd"])
        line_pct = parse_rust_line_coverage(output)
        print(f"{name} line coverage: {line_pct:.2f}% (minimum {target['minimum']:.2f}%)")
        if line_pct < target["minimum"]:
            failures.append(
                f"{name} line coverage {line_pct:.2f}% is below {target['minimum']:.2f}%"
            )

    run(
        [
            "cargo",
            "llvm-cov",
            "--json",
            "--summary-only",
            "-p",
            "buba-dashboard",
            "--lib",
            "--tests",
            "--bin",
            "buba-research-worker",
            "--output-path",
            str(ROOT / "target" / "llvm-cov" / "research-summary.json"),
        ]
    )
    research_rust_pct = parse_research_rust_line_coverage()
    print(
        f"research server line coverage: {research_rust_pct:.2f}% "
        f"(minimum {RESEARCH_RUST_MINIMUM:.2f}%)"
    )
    if research_rust_pct < RESEARCH_RUST_MINIMUM:
        failures.append(
            f"research server line coverage {research_rust_pct:.2f}% "
            f"is below {RESEARCH_RUST_MINIMUM:.2f}%"
        )

    run(["npm", "run", "test:coverage"], cwd=CLIENT_DIR)
    frontend_line_pct = parse_frontend_line_coverage()
    print(
        f"dashboard client line coverage: {frontend_line_pct:.2f}% "
        f"(minimum {FRONTEND_MINIMUM:.2f}%)"
    )
    if frontend_line_pct < FRONTEND_MINIMUM:
        failures.append(
            f"dashboard client line coverage {frontend_line_pct:.2f}% is below {FRONTEND_MINIMUM:.2f}%"
        )

    research_frontend_pct = parse_research_frontend_line_coverage()
    print(
        f"research client line coverage: {research_frontend_pct:.2f}% "
        f"(minimum {FRONTEND_RESEARCH_MINIMUM:.2f}%)"
    )
    if research_frontend_pct < FRONTEND_RESEARCH_MINIMUM:
        failures.append(
            f"research client line coverage {research_frontend_pct:.2f}% "
            f"is below {FRONTEND_RESEARCH_MINIMUM:.2f}%"
        )

    if failures:
        print("coverage gates failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("coverage gates passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
