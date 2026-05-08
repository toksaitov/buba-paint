#!/usr/bin/env python3
"""Reject known blocking operations from latency-sensitive runtime paths."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    """Return a repo file as UTF-8 text."""
    return (ROOT / path).read_text(encoding="utf-8")


def fail(message: str) -> None:
    """Print one audit failure."""
    print(f"hot-path audit: {message}", file=sys.stderr)


def live_runtime_body() -> str:
    """Return the live runtime source body used for coarse static checks."""
    source = read("bots/paint/src/live.rs")
    start = source.find("async fn run_live_runtime(")
    end = source.find("impl LiveTradingMonitor", start)
    if start == -1 or end == -1:
        fail("could not locate run_live_runtime body")
        sys.exit(1)
    return source[start:end]


def check_forbidden_live_runtime_calls(errors: list[str]) -> None:
    """Check that the main live runtime no longer performs known full scans."""
    body = live_runtime_body()
    forbidden = [
        "storage_footprint(",
        "persist_replay_quality_metadata(",
        "replay_quality::analyze_connection",
        "replay_quality::analyze_path",
        "PRAGMA quick_check",
        "monitor.fetch_account_state().await",
        "monitor.apply_pending_controls(",
        "monitor.refresh_remote_state(",
        "sidecar.account_state().await",
        "sidecar.preflight(",
        "sidecar.activity().await",
        "sidecar.submit_order_intent(",
        "db.upsert_market(",
        "db.set_run_metadata(",
        "db.log_feed_health_event(",
        "earliest_binance_price_in_window(",
        "runtime_capture_issues_for_live(",
        "pending_live_control_commands(",
    ]
    for pattern in forbidden:
        if pattern in body:
            errors.append(f"forbidden live runtime call remains: {pattern}")


def check_docker_healthchecks(errors: list[str]) -> None:
    """Check Docker healthchecks for O(N) SQLite diagnostics."""
    for path in sorted(ROOT.glob("docker-compose*.yml")):
        text = path.read_text(encoding="utf-8")
        if "quick_check" in text:
            errors.append(f"{path.name} uses SQLite quick_check in runtime compose")


def check_runtime_replay_metadata(errors: list[str]) -> None:
    """Check that live arming uses runtime metadata instead of replay scans."""
    text = read("bots/paint/src/live.rs")
    if "fn persist_replay_quality_metadata" in text:
        errors.append("live.rs still defines runtime replay-quality scan helper")
    if "fn replay_quality_issues_for_live" in text:
        errors.append("live.rs still defines replay-quality live gate scan helper")


def check_tick_logging_default(errors: list[str]) -> None:
    """Check that legacy tick logging stays opt-in for replay-grade runs."""
    text = read("bots/paint/src/config.rs")
    if 'tick_data_logging_enabled: env_bool("TICK_DATA_LOGGING_ENABLED", false)' not in text:
        errors.append("TICK_DATA_LOGGING_ENABLED is not default-false in env config")
    if "tick_data_logging_enabled: false" not in text:
        errors.append("Config::default does not disable tick_data logging")


def main() -> int:
    """Run the hot-path audit."""
    errors: list[str] = []
    check_forbidden_live_runtime_calls(errors)
    check_docker_healthchecks(errors)
    check_runtime_replay_metadata(errors)
    check_tick_logging_default(errors)
    if re.search(r"healthcheck:\s*\n(?:.*\n){0,8}.*quick_check", read("docker-compose.yml")):
        errors.append("docker-compose.yml healthcheck still references quick_check")
    if errors:
        for error in errors:
            fail(error)
        return 1
    print("hot-path audit passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
