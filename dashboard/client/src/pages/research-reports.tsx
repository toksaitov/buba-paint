import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  Banner,
  Button,
  Input,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchReports } from "../hooks/use-research-reports";
import {
  parseReportSummary,
  sortReports,
  type ReportSortKey,
} from "../lib/research-report-analysis";
import { reportTone } from "../lib/research-permissions";
import type { JobType, ReportStatus } from "../lib/research-types";
import { formatSignedUsd, humanize } from "../lib/utils";

const ALL_STATUSES: ReportStatus[] = ["available", "archived"];
const JOB_TYPES: Array<"all" | JobType | "unknown"> = [
  "all",
  "current_params",
  "sweep",
  "export",
  "unknown",
];
const SORT_OPTIONS: Array<{ value: ReportSortKey; label: string }> = [
  { value: "net_pnl_desc", label: "Net PnL" },
  { value: "updated_desc", label: "Updated" },
  { value: "drawdown_best", label: "Max drawdown" },
  { value: "win_rate_desc", label: "Win rate" },
  { value: "trades_desc", label: "Trades" },
];

export function ResearchReportsPage() {
  const navigate = useNavigate();
  const reportsQuery = useResearchReports();
  const [active, setActive] = useState<string[]>(["available"]);
  const [typeFilter, setTypeFilter] = useState<"all" | JobType | "unknown">(
    "all",
  );
  const [analysisFilter, setAnalysisFilter] = useState<
    "all" | "with" | "missing"
  >("all");
  const [artifactFilter, setArtifactFilter] = useState("");
  const [sortKey, setSortKey] = useState<ReportSortKey>("net_pnl_desc");
  const [selected, setSelected] = useState<string[]>([]);

  const reportsData = reportsQuery.data?.reports;
  const filtered = useMemo(
    () => {
      const artifactNeedle = artifactFilter.trim().toLowerCase();
      const visible = [...(reportsData ?? [])]
        .filter((r) => active.includes(r.status))
        .filter((r) => {
          const parsed = parseReportSummary(r);
          const jobType = parsed.provenance.job_type ?? "unknown";
          return typeFilter === "all" || jobType === typeFilter;
        })
        .filter((r) => {
          const parsed = parseReportSummary(r);
          if (analysisFilter === "with") return parsed.has_analysis;
          if (analysisFilter === "missing") return !parsed.has_analysis;
          return true;
        })
        .filter((r) => {
          if (!artifactNeedle) return true;
          return (r.artifact_id ?? "").toLowerCase().includes(artifactNeedle);
        });
      return sortReports(visible, sortKey);
    },
    [reportsData, active, typeFilter, analysisFilter, artifactFilter, sortKey],
  );
  const visibleSelected = selected.filter((id) =>
    filtered.some((report) => report.id === id),
  );

  if (reportsQuery.isLoading) {
    return <Loading label="Loading reports" />;
  }
  if (reportsQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load reports">
        {(reportsQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard
        title="All reports"
        toolbar={
          <Button
            size="sm"
            disabled={visibleSelected.length < 2}
            onClick={() =>
              navigate(
                `/research/reports/compare?ids=${visibleSelected
                  .map(encodeURIComponent)
                  .join(",")}`,
              )
            }
          >
            Compare selected
          </Button>
        }
      >
        <div className="mb-3 grid gap-2 md:grid-cols-4">
          <select
            aria-label="Report job type filter"
            value={typeFilter}
            onChange={(e) =>
              setTypeFilter(e.currentTarget.value as typeof typeFilter)
            }
            className="border border-border bg-bg px-2 py-1 text-[11px]"
          >
            {JOB_TYPES.map((type) => (
              <option key={type} value={type}>
                {type === "all" ? "All job types" : humanize(type)}
              </option>
            ))}
          </select>
          <select
            aria-label="Report analysis filter"
            value={analysisFilter}
            onChange={(e) =>
              setAnalysisFilter(e.currentTarget.value as typeof analysisFilter)
            }
            className="border border-border bg-bg px-2 py-1 text-[11px]"
          >
            <option value="all">All analysis states</option>
            <option value="with">With analysis</option>
            <option value="missing">Missing analysis</option>
          </select>
          <select
            aria-label="Report sort"
            value={sortKey}
            onChange={(e) => setSortKey(e.currentTarget.value as ReportSortKey)}
            className="border border-border bg-bg px-2 py-1 text-[11px]"
          >
            {SORT_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                Sort: {opt.label}
              </option>
            ))}
          </select>
          <Input
            aria-label="Artifact filter"
            value={artifactFilter}
            onChange={(e) => setArtifactFilter(e.currentTarget.value)}
            placeholder="Filter artifact"
          />
        </div>
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => reportTone(s as ReportStatus)}
          ariaLabel="Report status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No reports yet — they appear after a backtest or sweep job completes." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">Compare</th>
                  <th className="px-2 py-1.5 font-semibold">Title</th>
                  <th className="px-2 py-1.5 font-semibold">Metrics</th>
                  <th className="px-2 py-1.5 font-semibold">Job</th>
                  <th className="px-2 py-1.5 font-semibold">Artifact</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((report) => {
                  const parsed = parseReportSummary(report);
                  const checked = selected.includes(report.id);
                  return (
                    <tr
                      key={report.id}
                      className="border-b border-border last:border-b-0 hover:bg-surface"
                    >
                      <td className="px-2 py-1.5">
                        <input
                          type="checkbox"
                          aria-label={`Compare ${report.title}`}
                          checked={checked}
                          onChange={(e) => {
                            const nextChecked = e.currentTarget.checked;
                            setSelected((current) =>
                              nextChecked
                                ? [...current, report.id]
                                : current.filter((id) => id !== report.id),
                            );
                          }}
                        />
                      </td>
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/reports/${encodeURIComponent(report.id)}`}
                          className="font-semibold hover:underline"
                        >
                          {report.title}
                        </Link>
                        <div className="font-mono text-[11px] text-muted">
                          {report.id}
                        </div>
                        <div className="text-[11px] text-muted">
                          {parsed.provenance.job_type
                            ? humanize(parsed.provenance.job_type)
                            : "Analysis unavailable"}
                        </div>
                      </td>
                      <td className="px-2 py-1.5 text-[11px]">
                        {parsed.has_analysis ? (
                          <div className="space-y-0.5">
                            <div>
                              Net PnL{" "}
                              <span className="font-semibold">
                                {formatMetricUsd(parsed.metrics.net_pnl)}
                              </span>
                            </div>
                            <div className="text-muted">
                              DD {formatMetricUsd(parsed.metrics.max_drawdown)} ·
                              WR {formatPercent(parsed.metrics.win_rate)} ·
                              Trades {formatInteger(parsed.metrics.trade_count)}
                            </div>
                          </div>
                        ) : (
                          <span className="text-muted">Analysis unavailable</span>
                        )}
                      </td>
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/jobs/${encodeURIComponent(report.job_id)}`}
                          className="font-mono text-[11px] hover:underline"
                        >
                          {report.job_id}
                        </Link>
                      </td>
                      <td className="px-2 py-1.5">
                        {report.artifact_id ? (
                          <Link
                            to={`/research/artifacts/${encodeURIComponent(report.artifact_id)}`}
                            className="font-mono text-[11px] hover:underline"
                          >
                            {report.artifact_id}
                          </Link>
                        ) : (
                          <span className="text-muted">—</span>
                        )}
                      </td>
                      <td className="px-2 py-1.5">
                        <StatusChip
                          label={humanize(report.status)}
                          tone={reportTone(report.status)}
                          compact
                        />
                      </td>
                      <td className="px-2 py-1.5 text-muted">
                        <RelativeTime epochMs={report.updated_at} />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
    </div>
  );
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
