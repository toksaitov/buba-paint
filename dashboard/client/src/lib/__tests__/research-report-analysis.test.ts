import { describe, expect, it } from "vitest";
import type { ResearchReport } from "../research-types";
import {
  bestReportLabel,
  comparisonWarnings,
  parseReportPayload,
  parseReportSummary,
  sortReports,
} from "../research-report-analysis";

function report(
  id: string,
  netPnl: number | null,
  overrides: Record<string, unknown> = {},
): ResearchReport {
  const summary = {
    schema_version: 2,
    provenance: {
      job_id: `job-${id}`,
      job_type: "current_params",
      artifact_id: "artifact-a",
      start_ms: 1,
      end_ms: 2,
      balance: 200,
      ...((overrides.provenance as Record<string, unknown> | undefined) ?? {}),
    },
    metrics: {
      net_pnl: netPnl,
      max_drawdown: -1,
      win_rate: 0.5,
      trade_count: 2,
      ...((overrides.metrics as Record<string, unknown> | undefined) ?? {}),
    },
    diagnostics: [],
  };
  return {
    id,
    job_id: `job-${id}`,
    artifact_id: "artifact-a",
    title: `Report ${id}`,
    status: "available",
    summary_json: JSON.stringify(summary),
    report_path: `/reports/${id}.json`,
    csv_path: `/reports/${id}.csv`,
    created_at: 1,
    updated_at: id === "a" ? 10 : 20,
  };
}

describe("research report analysis helpers", () => {
  it("sorts reports by Net PnL first", () => {
    const sorted = sortReports([report("a", 1), report("b", 5)], "net_pnl_desc");

    expect(sorted.map((r) => r.id)).toEqual(["b", "a"]);
  });

  it("parses legacy flat metric summaries without marking analysis available", () => {
    const parsed = parseReportSummary({
      ...report("legacy", null),
      summary_json: JSON.stringify({ net_pnl: 4, trade_count: 1 }),
    });

    expect(parsed.has_analysis).toBe(false);
    expect(parsed.metrics.net_pnl).toBe(4);
  });

  it("warns when compared reports differ in provenance", () => {
    const a = report("a", 1);
    const b = report("b", 2, {
      provenance: { artifact_id: "artifact-b", start_ms: 3 },
    });
    const parsed = [
      { report: a, parsed: parseReportPayload(JSON.parse(a.summary_json!), a) },
      { report: b, parsed: parseReportPayload(JSON.parse(b.summary_json!), b) },
    ];

    expect(comparisonWarnings(parsed)).toContain("Reports use different artifacts.");
    expect(comparisonWarnings(parsed)).toContain("Reports use different start times.");
  });

  it("reports a tie instead of inventing a winner", () => {
    const a = report("a", 5);
    const b = report("b", 5);
    const parsed = [
      { report: a, parsed: parseReportPayload(JSON.parse(a.summary_json!), a) },
      { report: b, parsed: parseReportPayload(JSON.parse(b.summary_json!), b) },
    ];

    expect(bestReportLabel(parsed)).toMatch(/tied/i);
  });
});
