import { useMemo } from "react";
import {
  Link,
  useLocation,
  useNavigate,
  useSearchParams,
} from "react-router-dom";
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
import { useRememberResearchListReturn } from "../hooks/use-research-return-to";
import {
  netPnlMetricLabel,
  parseReportSummary,
  reportHasSourceMismatch,
  sortReports,
  type ReportSortKey,
} from "../lib/research-report-analysis";
import { reportTone } from "../lib/research-permissions";
import type { JobType, ReportStatus } from "../lib/research-types";
import {
  readEnumListParam,
  readEnumParam,
  readTextParam,
  sameEnumSet,
  setQueryListParam,
  setQueryParam,
  updateQueryParam,
} from "../lib/research-list-url-state";
import { formatSignedUsd, humanize, truncateMiddle } from "../lib/utils";

const ALL_STATUSES: ReportStatus[] = ["available", "archived"];
const JOB_TYPES: Array<"all" | JobType | "unknown"> = [
  "all",
  "current_params",
  "sweep",
  "export",
  "unknown",
];
const SORT_OPTIONS: Array<{ value: ReportSortKey; label: string }> = [
  { value: "net_pnl_desc", label: "Report PnL" },
  { value: "updated_desc", label: "Updated" },
  { value: "drawdown_best", label: "Max drawdown" },
  { value: "win_rate_desc", label: "Win rate" },
  { value: "trades_desc", label: "Trades" },
];
const DEFAULT_STATUSES: ReportStatus[] = ["available"];
const DEFAULT_SORT: ReportSortKey = "net_pnl_desc";
const ANALYSIS_FILTERS = ["all", "with", "missing"] as const;
const RETENTION_FILTERS = ["all", "archive_candidate", "archived"] as const;

export function ResearchReportsPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  useRememberResearchListReturn("reports", "/research/reports");
  const returnToReports = `${location.pathname}${location.search}`;
  const reportsQuery = useResearchReports();
  const typeFilter = readEnumParam(searchParams, "type", JOB_TYPES, "all");
  const analysisFilter = readEnumParam(
    searchParams,
    "analysis",
    ANALYSIS_FILTERS,
    "all",
  );
  const rawRetentionFilter = readEnumParam(
    searchParams,
    "retention",
    RETENTION_FILTERS,
    "all",
  );
  const active = readEnumListParam(
    searchParams,
    "status",
    ALL_STATUSES,
    reportRetentionStatuses(rawRetentionFilter),
  );
  const retentionFilter =
    searchParams.has("status") &&
    !sameEnumSet(active, reportRetentionStatuses(rawRetentionFilter))
      ? "all"
      : rawRetentionFilter;
  const legacyArtifactFilter = readTextParam(searchParams, "artifact");
  const textFilter =
    readTextParam(searchParams, "q") || legacyArtifactFilter;
  const sortKey = readEnumParam(
    searchParams,
    "sort",
    SORT_OPTIONS.map((option) => option.value),
    DEFAULT_SORT,
  );
  const selected = useMemo(
    () => readSelectedReportIds(searchParams),
    [searchParams],
  );

  const reportsData = reportsQuery.data?.reports;
  const filtered = useMemo(
    () => {
      const searchNeedle = textFilter.trim().toLowerCase();
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
          if (retentionFilter === "archive_candidate") {
            return r.status === "available";
          }
          if (retentionFilter === "archived") return r.status === "archived";
          return true;
        })
        .filter((r) => {
          if (!searchNeedle) return true;
          const parsed = parseReportSummary(r);
          return [
            r.title,
            r.id,
            r.job_id,
            r.artifact_id ?? "",
            parsed.provenance.job_type ? humanize(parsed.provenance.job_type) : "",
          ]
            .join(" ")
            .toLowerCase()
            .includes(searchNeedle);
        });
      return sortReports(visible, sortKey);
    },
    [
      reportsData,
      active,
      typeFilter,
      analysisFilter,
      retentionFilter,
      textFilter,
      sortKey,
    ],
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
        <div className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-5">
          <select
            aria-label="Report job type filter"
            value={typeFilter}
            onChange={(e) =>
              updateQueryParam(
                searchParams,
                setSearchParams,
                "type",
                e.currentTarget.value,
                "all",
              )
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
              updateQueryParam(
                searchParams,
                setSearchParams,
                "analysis",
                e.currentTarget.value,
                "all",
              )
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
            onChange={(e) =>
              updateQueryParam(
                searchParams,
                setSearchParams,
                "sort",
                e.currentTarget.value,
                DEFAULT_SORT,
              )
            }
            className="border border-border bg-bg px-2 py-1 text-[11px]"
          >
            {SORT_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                Sort: {opt.label}
              </option>
            ))}
          </select>
          <select
            aria-label="Report retention filter"
            value={retentionFilter}
            onChange={(e) => {
              const nextRetention = e.currentTarget
                .value as (typeof RETENTION_FILTERS)[number];
              const next = new URLSearchParams(searchParams);
              setQueryParam(next, "retention", nextRetention, "all");
              setQueryListParam(
                next,
                "status",
                reportRetentionStatuses(nextRetention),
                reportRetentionStatuses(nextRetention),
              );
              setSearchParams(next, { replace: true });
            }}
            className="border border-border bg-bg px-2 py-1 text-[11px]"
          >
            <option value="all">All retention states</option>
            <option value="archive_candidate">Archive candidates</option>
            <option value="archived">Archived only</option>
          </select>
          <div className="col-span-2 md:col-span-1">
            <Input
              aria-label="Search reports"
              value={textFilter}
              onChange={(e) => {
                const next = new URLSearchParams(searchParams);
                setQueryParam(next, "q", e.currentTarget.value, "");
                next.delete("artifact");
                setSearchParams(next, { replace: true });
              }}
              placeholder="Search report, job, artifact"
            />
          </div>
        </div>
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={(next) => {
            const params = new URLSearchParams(searchParams);
            setQueryParam(params, "retention", "all", "all");
            setQueryListParam(
              params,
              "status",
              next,
              reportRetentionStatuses("all"),
            );
            setSearchParams(params, { replace: true });
          }}
          toneFor={(s) => reportTone(s as ReportStatus)}
          ariaLabel="Report status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty
            message={
              (reportsQuery.data?.reports ?? []).length === 0
                ? "No reports yet. They appear after a backtest or sweep job completes."
                : "No reports match the selected filters."
            }
          />
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
                            const nextSelected = nextChecked
                              ? [...selected, report.id]
                              : selected.filter((id) => id !== report.id);
                            const next = new URLSearchParams(searchParams);
                            setSelectedReportIds(next, nextSelected);
                            setSearchParams(next, { replace: true });
                          }}
                        />
                      </td>
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/reports/${encodeURIComponent(report.id)}`}
                          state={{ returnTo: returnToReports }}
                          className="font-semibold hover:underline"
                        >
                          {report.title}
                        </Link>
                        <div
                          title={report.id}
                          className="font-mono text-[11px] text-muted"
                        >
                          {truncateMiddle(report.id, 8, 4)}
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
                              {netPnlMetricLabel(parsed)}{" "}
                              <span className="font-semibold">
                                {formatMetricUsd(parsed.metrics.net_pnl)}
                              </span>
                            </div>
                            <div className="text-muted">
                              DD {formatMetricUsd(parsed.metrics.max_drawdown)} ·
                              WR {formatPercent(parsed.metrics.win_rate)} ·
                              Trades {formatInteger(parsed.metrics.trade_count)}
                            </div>
                            {reportHasSourceMismatch(parsed) && (
                              <div className="flex flex-wrap items-center gap-1.5">
                                <StatusChip
                                  label="Source mismatch"
                                  tone="warning"
                                  compact
                                />
                                <span className="text-muted">
                                  Source run{" "}
                                  {formatMetricUsd(
                                    parsed.source_comparison?.source.net_pnl,
                                  )}{" "}
                                  Delta{" "}
                                  {formatMetricUsd(
                                    parsed.source_comparison?.delta.net_pnl,
                                  )}
                                </span>
                              </div>
                            )}
                          </div>
                        ) : (
                          <span className="text-muted">Analysis unavailable</span>
                        )}
                      </td>
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/jobs/${encodeURIComponent(report.job_id)}`}
                          title={report.job_id}
                          className="whitespace-nowrap font-mono text-[11px] hover:underline"
                        >
                          {truncateMiddle(report.job_id, 8, 4)}
                        </Link>
                      </td>
                      <td className="px-2 py-1.5">
                        {report.artifact_id ? (
                          <Link
                            to={`/research/artifacts/${encodeURIComponent(report.artifact_id)}`}
                            title={report.artifact_id}
                            className="whitespace-nowrap font-mono text-[11px] hover:underline"
                          >
                            {truncateMiddle(report.artifact_id, 14, 8)}
                          </Link>
                        ) : (
                          <span className="text-muted">-</span>
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

function reportRetentionStatuses(
  retention: (typeof RETENTION_FILTERS)[number],
): ReportStatus[] {
  if (retention === "archived") return ["archived"];
  return DEFAULT_STATUSES;
}

function readSelectedReportIds(params: URLSearchParams): string[] {
  const value = params.get("selected");
  if (!value) return [];
  return value
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean);
}

function setSelectedReportIds(params: URLSearchParams, ids: readonly string[]) {
  if (ids.length === 0) {
    params.delete("selected");
  } else {
    params.set("selected", [...new Set(ids)].join(","));
  }
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
