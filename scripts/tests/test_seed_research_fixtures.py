"""Unit tests for the research fixture seed script."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = str(REPO_ROOT / "scripts")
if SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, SCRIPTS_DIR)


def load_script(name: str, path: Path):
    """Load one script as a module."""
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SEED = load_script(
    "seed_research_fixtures",
    REPO_ROOT / "scripts" / "seed-research-fixtures.py",
)

CANONICAL_PROVENANCE_KEYS = {
    "job_id",
    "job_type",
    "artifact_id",
    "start",
    "end",
    "start_ms",
    "end_ms",
    "balance",
    "sets",
    "sweeps",
    "dashboard_image_ref",
    "research_worker_image_ref",
}

CANONICAL_METRIC_KEYS = {
    "net_pnl",
    "gross_pnl",
    "total_fees",
    "final_balance",
    "trade_count",
    "wins",
    "losses",
    "win_rate",
    "max_drawdown",
    "max_drawdown_pct",
    "signal_count",
    "fill_count",
    "no_fill_count",
}


class CanonicalReportPayloadTests(unittest.TestCase):
    """Pin the canonical schema_version 2 fixture shape."""

    def test_report_payload_is_schema_version_2(self) -> None:
        """The promoted payload is a schema_version 2 document."""
        payload = SEED.report_payload(
            "fixture-report-available", "fixture-job-completed", "available"
        )
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["source_comparison"]["status"], "match")
        self.assertEqual(len(payload["equity_curve"]), 5)
        self.assertEqual(len(payload["drawdown_curve"]), 5)

    def test_report_payload_has_canonical_keys(self) -> None:
        """Provenance and metrics expose the full canonical key set."""
        payload = SEED.report_payload(
            "fixture-report-available", "fixture-job-completed", "available"
        )
        self.assertTrue(
            CANONICAL_PROVENANCE_KEYS.issubset(payload["provenance"].keys())
        )
        self.assertTrue(CANONICAL_METRIC_KEYS.issubset(payload["metrics"].keys()))

    def test_summary_payload_drops_chart_arrays(self) -> None:
        """The DB summary mirrors the Rust summary, without chart arrays."""
        summary = SEED.report_summary_payload(
            "fixture-report-available", "fixture-job-completed", "available"
        )
        self.assertEqual(summary["schema_version"], 2)
        self.assertNotIn("equity_curve", summary)
        self.assertNotIn("drawdown_curve", summary)
        self.assertIn("metrics", summary)

    def test_legacy_payload_retained(self) -> None:
        """Exactly one legacy schema_version 1 fixture is retained."""
        legacy = SEED.legacy_report_payload(
            "fixture-report-missing-file", "fixture-job-blocked", "available"
        )
        self.assertEqual(legacy["schema_version"], 1)
        self.assertTrue(legacy["fixture"])

    def test_python_ts_metric_parity(self) -> None:
        """Python metric keys match the canonical TS fixture metric keys."""
        payload = SEED.report_payload(
            "fixture-report-available", "fixture-job-completed", "available"
        )
        self.assertEqual(set(payload["metrics"].keys()), CANONICAL_METRIC_KEYS)
        self.assertEqual(set(payload["provenance"].keys()), CANONICAL_PROVENANCE_KEYS)


if __name__ == "__main__":
    unittest.main()
