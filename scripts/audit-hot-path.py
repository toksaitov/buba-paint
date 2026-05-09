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
    if 'db.set_run_metadata("replay_quality_class"' in text:
        errors.append("live runtime writes offline replay_quality_class metadata")


def check_legacy_live_submission_absent(errors: list[str]) -> None:
    """Check that old direct live-submission methods cannot be resurrected."""
    text = read("bots/paint/src/live.rs")
    forbidden = [
        "async fn submit_orders(",
        "async fn submit_one_order(",
        "fn persist_live_order_intent(\n",
        "fn build_live_order_request(\n",
        "fn handle_live_order_response(\n",
        "async fn handle_live_order_error(\n",
    ]
    for pattern in forbidden:
        if pattern in text:
            errors.append(f"legacy direct live-submission method remains: {pattern.strip()}")


def check_decision_evidence_wired(errors: list[str]) -> None:
    """Check that compact decision evidence is not left as dead code."""
    text = read("bots/paint/src/live_decision.rs")
    if "#[allow(dead_code)]\nfn decision_evidence_json" in text:
        errors.append("decision evidence helper is still marked dead code")
    if "decision_signal_event(" not in text:
        errors.append("decision evidence is not wired into signal persistence")
    required_fields = [
        '"config_fingerprint"',
        '"market"',
        '"bankroll"',
        '"exposure"',
        '"pending_order_count"',
        '"decision_sequence"',
        '"decision_input_at_us"',
    ]
    for field in required_fields:
        if field not in text:
            errors.append(f"decision evidence is missing {field}")


def check_decision_worker_purity(errors: list[str]) -> None:
    """Check that the runtime decision engine has no database or venue side effects."""
    text = read("bots/paint/src/live_decision.rs")
    forbidden = [
        "Database",
        "rusqlite",
        ".await",
        "LiveSidecarClient",
        "sidecar",
        "run_strategy_cycle(",
        "ExecutionEngine",
        "log_signal",
        ".open_trade(",
        ".close_trade(",
        "get_open_trades",
        "count_open_trades",
        "quick_check",
        "storage_footprint(",
        "validate-replay-data",
        "validate_live_fidelity",
    ]
    for pattern in forbidden:
        if pattern in text:
            errors.append(f"runtime decision engine contains forbidden pattern: {pattern}")


def check_legacy_strategy_cycle_absent_from_live(errors: list[str]) -> None:
    """Check that live runtime no longer routes decisions through DB-backed cycle code."""
    text = read("bots/paint/src/live.rs")
    forbidden = [
        "run_strategy_cycle(",
        "ExecutionEngine::new(",
        "process_due_orders_and_log(",
        "log_execution_rollup_for_market(",
        "handle_authoritative_resolution(",
    ]
    for pattern in forbidden:
        if pattern in text:
            errors.append(f"live runtime contains legacy DB-backed decision pattern: {pattern}")


def check_direct_venue_submission_from_runtime(errors: list[str]) -> None:
    """Check venue submissions are isolated outside the main feed runtime body."""
    body = live_runtime_body()
    forbidden = [
        "submit_order_intent(",
        "cancel_all().await",
        "redeem_all().await",
    ]
    for pattern in forbidden:
        if pattern in body:
            errors.append(f"main feed runtime performs direct venue mutation: {pattern}")


def check_live_worker_boundaries(errors: list[str]) -> None:
    """Check live workers keep evidence, control, and shutdown boundaries explicit."""
    live = read("bots/paint/src/live.rs")
    feed_writer = read("bots/paint/src/live_feed_writer.rs")
    persistence_writer = read("bots/paint/src/live_persistence_writer.rs")
    submission_start = re.search(
        r"impl LiveSubmissionQueue \{(?P<body>.*?)/// Queue one live submission batch",
        live,
        re.S,
    )
    if submission_start and "feed_event_writer_queue_capacity" in submission_start.group("body"):
        errors.append("live submission queue still reuses feed writer capacity")
    strategy_start = re.search(
        r"impl StrategyWorker \{(?P<body>.*?)/// Enqueue one strategy-evaluation request",
        live,
        re.S,
    )
    if strategy_start and "feed_event_writer_queue_capacity" in strategy_start.group("body"):
        errors.append("strategy decision queue still reuses feed writer capacity")
    persistence_start = re.search(
        r"LivePersistenceWriter::start\((?P<body>.*?)\)\?;",
        live,
        re.S,
    )
    if persistence_start and "feed_event_writer_queue_capacity" in persistence_start.group("body"):
        errors.append("runtime persistence queue still reuses feed writer capacity")
    if "unbounded_channel::<StrategyWorkerOutput>" in live:
        errors.append("strategy decision output channel is unbounded")
    if "fn stale_live_decision_output(" not in live:
        errors.append("live runtime lacks stale-decision output gate")
    if "max_live_decision_age_ms" not in live:
        errors.append("live runtime does not apply max live decision age")
    if "let mut live_monitor_inflight" in live:
        errors.append("live monitor controls and remote refresh still share one inflight slot")
    if "critical_events: Vec<LivePersistenceEvent>" not in live:
        errors.append("live submission requests do not carry critical decision evidence")
    if "log_live_order_intent_with_decision_evidence" not in live:
        errors.append("live submission worker does not persist evidence with order intent")
    forbidden_db_spawn_helpers = [
        "spawn_feed_health_event_write",
        "spawn_runtime_capture_metadata_write",
        "spawn_market_upsert",
        "persist_owned_feed_health_event",
        "persist_runtime_capture_metadata_worker",
        "persist_market_upsert",
    ]
    for pattern in forbidden_db_spawn_helpers:
        if pattern in live:
            errors.append(f"ad-hoc runtime DB task remains: {pattern}")
    for path, text in {
        "live.rs": live,
        "live_feed_writer.rs": feed_writer,
        "live_persistence_writer.rs": persistence_writer,
    }.items():
        if re.search(r"\.tx\.send\([^)]*Shutdown", text):
            errors.append(f"{path} uses blocking shutdown send")
        if ".join()" in text and "join_with_timeout" not in text:
            errors.append(f"{path} uses unbounded worker join")
    for path, text in {
        "live_feed_writer.rs": feed_writer,
        "live_persistence_writer.rs": persistence_writer,
    }.items():
        if "Database::new(" in text:
            errors.append(f"{path} runs migrations from a runtime worker")
    if "LiveSidecarClient::new(&config.live_sidecar_url)" in live:
        errors.append("live runtime builds sidecar client without configured timeouts")


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
    check_legacy_live_submission_absent(errors)
    check_decision_evidence_wired(errors)
    check_decision_worker_purity(errors)
    check_legacy_strategy_cycle_absent_from_live(errors)
    check_direct_venue_submission_from_runtime(errors)
    check_live_worker_boundaries(errors)
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
