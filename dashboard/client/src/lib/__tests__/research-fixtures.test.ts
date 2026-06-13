import { describe, expect, it } from "vitest";
import {
  fixtureReportAvailable,
  fixtureReportJsonPayload,
  fixtureReportLegacyJsonPayload,
  fixtureReportMissingFile,
  fixtureReportSummaryJsonPayload,
} from "../research-fixtures";
import {
  parseReportPayload,
  parseReportSummary,
} from "../research-report-analysis";

const CANONICAL_PROVENANCE_KEYS = [
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
];

const CANONICAL_METRIC_KEYS = [
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
];

describe("research report fixtures", () => {
  it("promotes the shared report payload to a parseable schema_version 2 document", () => {
    const parsed = parseReportPayload(fixtureReportJsonPayload());

    expect(parsed.schema_version).toBe(2);
    expect(parsed.has_analysis).toBe(true);
    expect(parsed.metrics.net_pnl).toBe(284.25);
    expect(parsed.source_comparison?.status).toBe("match");
    expect(parsed.equity_curve.length).toBe(5);
    expect(parsed.drawdown_curve.length).toBe(5);
    expect(parsed.rejection_reasons.length).toBe(2);
  });

  it("exposes every canonical provenance and metric key", () => {
    const payload = fixtureReportJsonPayload();
    const provenance = payload.provenance as Record<string, unknown>;
    const metrics = payload.metrics as Record<string, unknown>;

    for (const key of CANONICAL_PROVENANCE_KEYS) {
      expect(provenance).toHaveProperty(key);
    }
    for (const key of CANONICAL_METRIC_KEYS) {
      expect(metrics).toHaveProperty(key);
    }
  });

  it("keeps the summary payload analysis-capable without chart arrays", () => {
    const summary = fixtureReportSummaryJsonPayload();

    expect(summary).not.toHaveProperty("equity_curve");
    expect(summary).not.toHaveProperty("drawdown_curve");
    expect(parseReportSummary(fixtureReportAvailable()).has_analysis).toBe(true);
  });

  it("retains one explicit legacy fixture", () => {
    const legacy = parseReportSummary(fixtureReportMissingFile());

    expect(legacy.schema_version).toBeNull();
    expect(legacy.has_analysis).toBe(false);
    expect(legacy.metrics.net_pnl).toBe(284.25);
    expect(fixtureReportLegacyJsonPayload()).toHaveProperty("fixture", true);
  });
});
