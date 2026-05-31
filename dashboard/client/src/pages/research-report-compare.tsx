import { Link, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Banner,
  KeyValueList,
  MetricCard,
  SectionCard,
  StateEmpty,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import {
  getResearchReport,
  getResearchReportJson,
} from "../lib/research-api";
import {
  bestReportLabel,
  comparisonWarnings,
  parseReportPayload,
} from "../lib/research-report-analysis";
import { formatDateTime, formatSignedUsd, humanize } from "../lib/utils";

export function ResearchReportComparePage() {
  const [params] = useSearchParams();
  const returnToReports = useResearchReturnTo("reports", "/research/reports");
  const ids = (params.get("ids") ?? "")
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean);
  const uniqueIds = Array.from(new Set(ids));
  const comparisonQuery = useQuery({
    queryKey: ["research", "reports", "compare", uniqueIds],
    queryFn: async () =>
      Promise.all(
        uniqueIds.map(async (id) => {
          const report = await getResearchReport(id);
          const payload = await getResearchReportJson(id);
          return { report, parsed: parseReportPayload(payload, report) };
        }),
      ),
    enabled: uniqueIds.length >= 2,
    retry: false,
  });

  if (uniqueIds.length < 2) {
    return (
      <StateEmpty message="Select at least two reports from Research > Reports to compare." />
    );
  }
  if (comparisonQuery.isLoading) {
    return <Loading label="Loading comparison" />;
  }
  if (comparisonQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load comparison">
        {(comparisonQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  const loaded = comparisonQuery.data ?? [];
  const ranked = [...loaded].sort(
    (a, b) =>
      metricForSort(b.parsed.metrics.net_pnl) -
      metricForSort(a.parsed.metrics.net_pnl),
  );
  const warnings = comparisonWarnings(ranked);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToReports}
          className="text-[12px] text-muted hover:underline"
        >
          ← Reports
        </Link>
        <span className="text-[14px] font-semibold">Report comparison</span>
      </div>

      {warnings.length > 0 && (
        <Banner tone="warning" title="Compatibility warnings">
          {warnings.join(" ")}
        </Banner>
      )}

      <SectionCard title={bestReportLabel(ranked)}>
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {ranked.map(({ report, parsed }, index) => (
            <div key={report.id} className="border border-border bg-bg p-3">
              <div className="mb-2 flex items-center justify-between gap-2">
                <Link
                  to={`/research/reports/${encodeURIComponent(report.id)}`}
                  className="font-semibold hover:underline"
                >
                  {index + 1}. {report.title}
                </Link>
                <span className="font-mono text-[11px] text-muted">
                  {parsed.has_analysis ? "schema v2" : "legacy"}
                </span>
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                <MetricCard
                  label="Net PnL"
                  value={formatMetricUsd(parsed.metrics.net_pnl)}
                  tone={
                    parsed.metrics.net_pnl == null
                      ? "neutral"
                      : parsed.metrics.net_pnl >= 0
                        ? "success"
                        : "danger"
                  }
                />
                <MetricCard
                  label="Max drawdown"
                  value={formatMetricUsd(parsed.metrics.max_drawdown)}
                  tone="warning"
                />
                <MetricCard
                  label="Win rate"
                  value={formatPercent(parsed.metrics.win_rate)}
                />
                <MetricCard
                  label="Trades"
                  value={formatInteger(parsed.metrics.trade_count)}
                />
              </div>
              <KeyValueList
                columns={1}
                items={[
                  {
                    label: "Job type",
                    value: parsed.provenance.job_type
                      ? humanize(parsed.provenance.job_type)
                      : "—",
                  },
                  {
                    label: "Artifact",
                    value: parsed.provenance.artifact_id ?? "—",
                  },
                  {
                    label: "Interval",
                    value:
                      parsed.provenance.start && parsed.provenance.end
                        ? `${parsed.provenance.start} → ${parsed.provenance.end}`
                        : "—",
                  },
                  {
                    label: "Updated",
                    value: formatDateTime(report.updated_at),
                  },
                ]}
              />
              {parsed.diagnostics.length > 0 && (
                <div className="mt-2 text-[11px] text-muted">
                  Diagnostics: {parsed.diagnostics.join(", ")}
                </div>
              )}
            </div>
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

function metricForSort(value: number | null | undefined): number {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : Number.NEGATIVE_INFINITY;
}

function formatMetricUsd(value: number | null | undefined): string {
  return typeof value === "number" ? formatSignedUsd(value) : "n/a";
}

function formatPercent(value: number | null | undefined): string {
  return typeof value === "number" ? `${(value * 100).toFixed(1)}%` : "n/a";
}

function formatInteger(value: number | null | undefined): string {
  return typeof value === "number" ? value.toLocaleString() : "n/a";
}
