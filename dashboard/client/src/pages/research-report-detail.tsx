import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  Banner,
  Button,
  FormField,
  InfoHint,
  Input,
  KeyValueList,
  MetricCard,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { CsvPreview } from "../components/research/csv-preview";
import { JsonViewer } from "../components/research/json-viewer";
import {
  useResearchReport,
  useResearchReportCsv,
  useResearchReportJson,
} from "../hooks/use-research-reports";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useAuthStore } from "../stores/auth-store";
import { useTheme } from "../hooks/use-theme";
import { getChartColors } from "../lib/chart-colors";
import {
  archiveResearchReport,
  deleteResearchReport,
  restoreResearchReport,
  updateResearchReport,
} from "../lib/research-api";
import {
  ACTION_LABELS,
  canPerform,
  getActionGateState,
  getAllowedActions,
  jobTypeLabel,
  permissionHint,
  reportTone,
} from "../lib/research-permissions";
import {
  formatInteger,
  formatMetricUsd,
  formatPercent,
  formatUsd,
  netPnlMetricLabel,
  parseReportPayload,
  parseReportSummary,
} from "../lib/research-report-analysis";
import type {
  ResearchAction,
  UpdateReportRequest,
} from "../lib/research-types";
import {
  formatChartTick,
  formatDateTime,
  formatSignedUsd,
  humanize,
  rankedByLabel,
} from "../lib/utils";

function formatProvenanceInterval(
  start: string | null | undefined,
  end: string | null | undefined,
): string {
  if (!start || !end) return "-";
  const startMs = Date.parse(start);
  const endMs = Date.parse(end);
  if (Number.isNaN(startMs) || Number.isNaN(endMs)) {
    return `${start} → ${end}`;
  }
  return `${formatDateTime(startMs)} → ${formatDateTime(endMs)}`;
}

export function ResearchReportDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;
  const returnToReports = useResearchReturnTo("reports", "/research/reports");
  const { theme } = useTheme();
  const colors = getChartColors(theme);

  const reportQuery = useResearchReport(id);
  const [showJson, setShowJson] = useState(false);
  const [showCsv, setShowCsv] = useState(false);
  const jsonQuery = useResearchReportJson(id, true);
  const csvQuery = useResearchReportCsv(id, showCsv);

  const [editTitle, setEditTitle] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<ResearchAction | null>(
    null,
  );
  const [confirmDeleteRecordOpen, setConfirmDeleteRecordOpen] =
    useState(false);
  const [confirmDeleteFilesOpen, setConfirmDeleteFilesOpen] = useState(false);

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
    queryClient.invalidateQueries({ queryKey: ["research", "report", id] });
  };

  const updateMutation = useMutation({
    mutationFn: (req: UpdateReportRequest) => updateResearchReport(id, req),
    onSuccess: () => {
      invalidate();
      setEditTitle(null);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const archiveMutation = useMutation({
    mutationFn: () => archiveResearchReport(id),
    onMutate: () => {
      setPendingAction("archive");
      setActionError(null);
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidate();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const restoreMutation = useMutation({
    mutationFn: () => restoreResearchReport(id),
    onMutate: () => {
      setPendingAction("restore");
      setActionError(null);
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidate();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const deleteRecordMutation = useMutation({
    mutationFn: () => deleteResearchReport(id, false),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
      navigate(returnToReports);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const deleteFilesMutation = useMutation({
    mutationFn: () => deleteResearchReport(id, true),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
      navigate(returnToReports);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  if (reportQuery.isLoading) {
    return <Loading label="Loading report" />;
  }
  if (reportQuery.isError || !reportQuery.data) {
    return (
      <Banner tone="danger" title="Could not load report">
        {(reportQuery.error as Error)?.message ?? "Report not found."}
      </Banner>
    );
  }

  const report = reportQuery.data;
  const parsedSummary = parseReportSummary(report);
  const parsedPayload = jsonQuery.data
    ? parseReportPayload(jsonQuery.data, report)
    : null;
  const analysis = parsedPayload ?? parsedSummary;
  const metrics = analysis.metrics;
  const sourceComparison = analysis.source_comparison;
  const equity = parsedPayload?.equity_curve ?? [];
  const drawdown = parsedPayload?.drawdown_curve ?? [];
  const sweep = parsedPayload?.sweep ?? null;
  const rejectionReasons = parsedPayload?.rejection_reasons ?? [];
  const allowed = getAllowedActions("report", report.status);
  const canEditTitle = role ? canPerform(role, "update") : false;
  const filesAppearMissing = jsonQuery.isError && report.report_path != null;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToReports}
          className="text-[12px] text-muted hover:underline"
        >
          ← Reports
        </Link>
        <span className="text-[14px] font-semibold">{report.title}</span>
        <span className="font-mono text-[11px] text-muted">{report.id}</span>
        <StatusChip
          label={humanize(report.status)}
          tone={reportTone(report.status)}
        />
        {canEditTitle && editTitle === null && (
          <Button size="sm" onClick={() => setEditTitle(report.title)}>
            Rename
          </Button>
        )}
      </div>
      {editTitle !== null && (
        <SectionCard title="Rename report">
          <div className="space-y-3">
            <FormField label="Title">
              {({ id: fieldId }) => (
                <Input
                  id={fieldId}
                  value={editTitle}
                  onChange={(e) => setEditTitle(e.currentTarget.value)}
                  autoFocus
                />
              )}
            </FormField>
            <div className="flex justify-end gap-2">
              <Button onClick={() => setEditTitle(null)}>Cancel</Button>
              <Button
                tone="accent"
                disabled={!editTitle.trim() || updateMutation.isPending}
                state={updateMutation.isPending ? "pending" : "idle"}
                onClick={() =>
                  updateMutation.mutate({ title: editTitle.trim() })
                }
              >
                Save
              </Button>
            </div>
          </div>
        </SectionCard>
      )}

      {filesAppearMissing && (
        <Banner tone="danger" title="Report files appear to be missing">
          The report JSON at <span className="font-mono">{report.report_path}</span>{" "}
          could not be loaded. Open the source job and click{" "}
          <span className="font-semibold">Regenerate report</span> to rebuild
          the files.
        </Banner>
      )}
      {actionError && (
        <Banner tone="danger" title="Action failed">
          {actionError}
        </Banner>
      )}
      {!analysis.has_analysis && (
        <Banner tone="warning" title="Analysis metrics unavailable">
          This report was generated before schema v2 analysis, or the source
          output files were archived before regeneration. Raw JSON and CSV remain
          available when the files exist.
        </Banner>
      )}
      {analysis.diagnostics.includes("no_trades") && (
        <Banner tone="warning" title="No trades in this result">
          The run completed but produced no trades, so Net PnL comparison is not
          enough to choose a better configuration.
        </Banner>
      )}
      {sourceComparison?.status === "mismatch" && (
        <Banner tone="warning" title="Backtest differs from source run">
          Replay Net PnL is{" "}
          {formatMetricUsd(sourceComparison.replay.net_pnl)}, source run Net PnL
          is {formatMetricUsd(sourceComparison.source.net_pnl)}, and the delta
          is {formatMetricUsd(sourceComparison.delta.net_pnl)}. The charts below
          show replay output.
        </Banner>
      )}
      {sweep?.ranked_by === "calibrated_pnl" && (
        <Banner tone="info" title="Sweep uses calibrated ranking">
          Rows are ranked by source-baseline bias-adjusted PnL. Raw replay PnL
          remains available in the sweep table as `pnl`.
        </Banner>
      )}

      <SectionCard title="Provenance">
        <KeyValueList
          columns={2}
          items={[
            {
              label: "Source job",
              value: (
                <Link
                  to={`/research/jobs/${encodeURIComponent(report.job_id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {report.job_id}
                </Link>
              ),
            },
            {
              label: "Source artifact",
              value: report.artifact_id ? (
                <Link
                  to={`/research/artifacts/${encodeURIComponent(report.artifact_id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {report.artifact_id}
                </Link>
              ) : (
                "-"
              ),
            },
            {
              label: "Job type",
              value: analysis.provenance.job_type
                ? jobTypeLabel(analysis.provenance.job_type)
                : "-",
            },
            {
              label: "Interval",
              value: formatProvenanceInterval(
                analysis.provenance.start,
                analysis.provenance.end,
              ),
            },
            {
              label: "Starting balance",
              value:
                analysis.provenance.balance != null
                  ? formatSignedUsd(analysis.provenance.balance).replace("+", "")
                  : "-",
            },
            {
              label: "Report path",
              value: report.report_path ? (
                <span className="break-all font-mono text-[11px]">
                  {report.report_path}
                </span>
              ) : (
                "-"
              ),
            },
            {
              label: "CSV path",
              value: report.csv_path ? (
                <span className="break-all font-mono text-[11px]">
                  {report.csv_path}
                </span>
              ) : (
                "-"
              ),
            },
            {
              label: "Worker image",
              value: analysis.provenance.research_worker_image_ref ? (
                <span className="break-all font-mono text-[11px]">
                  {analysis.provenance.research_worker_image_ref}
                </span>
              ) : (
                "-"
              ),
            },
            {
              label: "Created",
              value: <RelativeTime epochMs={report.created_at} />,
            },
            {
              label: "Updated",
              value: <RelativeTime epochMs={report.updated_at} />,
            },
            {
              label: "Updated (absolute)",
              value: formatDateTime(report.updated_at),
            },
          ]}
        />
      </SectionCard>

      <SectionCard title="Summary metrics">
        <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">
          <MetricCard
            label={netPnlMetricLabel(analysis)}
            value={formatMetricUsd(metrics.net_pnl)}
            tone={
              metrics.net_pnl == null
                ? "neutral"
                : metrics.net_pnl >= 0
                  ? "success"
                  : "danger"
            }
          />
          <MetricCard
            label="Max drawdown"
            value={formatMetricUsd(metrics.max_drawdown)}
            tone="warning"
          />
          <MetricCard label="Win rate" value={formatPercent(metrics.win_rate)} />
          <MetricCard label="Trades" value={formatInteger(metrics.trade_count)} />
          <MetricCard
            label="Final balance"
            value={formatUsd(metrics.final_balance)}
          />
          {sourceComparison && (
            <>
              <MetricCard
                label="Source run Net PnL"
                value={formatMetricUsd(sourceComparison.source.net_pnl)}
                tone={
                  sourceComparison.source.net_pnl == null
                    ? "neutral"
                    : sourceComparison.source.net_pnl >= 0
                      ? "success"
                      : "danger"
                }
              />
              <MetricCard
                label="Replay delta"
                value={formatMetricUsd(sourceComparison.delta.net_pnl)}
                tone={
                  sourceComparison.status === "mismatch"
                    ? "warning"
                    : "neutral"
                }
              />
            </>
          )}
          <MetricCard
            label="Signals"
            value={formatInteger(metrics.signal_count)}
          />
          <MetricCard label="Wins" value={formatInteger(metrics.wins)} />
          <MetricCard label="Losses" value={formatInteger(metrics.losses)} />
        </div>
      </SectionCard>

      {sourceComparison && (
        <SectionCard title="Source run comparison">
          <KeyValueList
            columns={2}
            items={[
              {
                label: "Status",
                value: humanize(sourceComparison.status),
              },
              {
                label: "Replay Net PnL",
                value: formatMetricUsd(sourceComparison.replay.net_pnl),
              },
              {
                label: "Source Net PnL",
                value: formatMetricUsd(sourceComparison.source.net_pnl),
              },
              {
                label: "Net PnL delta",
                value: formatMetricUsd(sourceComparison.delta.net_pnl),
              },
              {
                label: "Replay final balance",
                value: formatUsd(sourceComparison.replay.final_balance),
              },
              {
                label: "Source final balance",
                value: formatUsd(sourceComparison.source.final_balance),
              },
              {
                label: "Final balance delta",
                value: formatMetricUsd(sourceComparison.delta.final_balance),
              },
              {
                label: "Replay trades",
                value: formatInteger(sourceComparison.replay.trade_count),
              },
              {
                label: "Source trades",
                value: formatInteger(sourceComparison.source.trade_count),
              },
              {
                label: "Trade delta",
                value: formatSignedInteger(sourceComparison.delta.trade_count),
              },
              {
                label: "Replay signals",
                value: formatInteger(sourceComparison.replay.signal_count),
              },
              {
                label: "Source signals",
                value: formatInteger(sourceComparison.source.signal_count),
              },
              {
                label: "Signal delta",
                value: formatSignedInteger(sourceComparison.delta.signal_count),
              },
            ]}
          />
        </SectionCard>
      )}

      {rejectionReasons.length > 0 && (
        <SectionCard title="Top rejection reasons">
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">Reason</th>
                  <th className="px-2 py-1.5 font-semibold">Count</th>
                </tr>
              </thead>
              <tbody>
                {rejectionReasons.map((reason) => (
                  <tr key={reason.reason} className="border-b border-border">
                    <td className="px-2 py-1.5 font-mono text-[11px]">
                      {reason.reason}
                    </td>
                    <td className="px-2 py-1.5 tabular-nums">
                      {reason.count.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </SectionCard>
      )}

      <SectionCard title="Equity and drawdown">
        {jsonQuery.isLoading ? (
          <Loading label="Loading report analysis" />
        ) : jsonQuery.isError ? (
          <Banner tone="danger" title="Could not load report JSON">
            {(jsonQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : equity.length > 0 ? (
          <div className="grid gap-3 lg:grid-cols-2">
            <MetricLineChart
              title="Equity curve"
              data={equity}
              dataKey="equity"
              colors={colors}
            />
            <MetricLineChart
              title="Drawdown"
              data={drawdown}
              dataKey="drawdown"
              colors={colors}
              zeroCeiling
            />
          </div>
        ) : (
          <StateEmpty message="No equity or drawdown series is available for this report." />
        )}
      </SectionCard>

      {sweep && (
        <SectionCard title={`Sweep ranking by ${rankedByLabel(sweep.ranked_by)}`}>
          {sweep.rows.length === 0 ? (
            <StateEmpty message="No sweep rows are available." />
          ) : (
            <div className="overflow-x-auto border border-border">
              <table className="w-full border-collapse text-[11px]">
                <thead className="border-b border-border bg-surface text-left text-[10px] uppercase text-muted">
                  <tr>
                    <th className="px-2 py-1 font-semibold">Rank</th>
                    {[...sweep.parameter_columns, ...sweep.metric_columns].map(
                      (column) => (
                        <th key={column} className="px-2 py-1 font-semibold">
                          {rankedByLabel(column)}
                        </th>
                      ),
                    )}
                  </tr>
                </thead>
                <tbody>
                  {sweep.rows.slice(0, 50).map((row) => (
                    <tr
                      key={row.rank ?? JSON.stringify(row.params)}
                      className="border-b border-border last:border-b-0"
                    >
                      <td className="px-2 py-1 tabular-nums">{row.rank}</td>
                      {sweep.parameter_columns.map((column) => (
                        <td key={column} className="px-2 py-1 tabular-nums">
                          {String(row.params[column] ?? "")}
                        </td>
                      ))}
                      {sweep.metric_columns.map((column) => (
                        <td key={column} className="px-2 py-1 tabular-nums">
                          {formatSweepMetric(column, row.metrics[column])}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </SectionCard>
      )}

      <SectionCard
        title="Report JSON"
        toolbar={
          <Button size="sm" onClick={() => setShowJson((s) => !s)}>
            {showJson ? "Hide JSON" : "Show JSON"}
          </Button>
        }
      >
        {!showJson ? (
          <StateEmpty message="Report JSON is hidden." />
        ) : jsonQuery.isLoading ? (
          <Loading label="Loading report JSON" />
        ) : jsonQuery.isError ? (
          <Banner tone="danger" title="Could not load report JSON">
            {(jsonQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : (
          <JsonViewer value={jsonQuery.data} label="Raw payload" maxHeight={300} />
        )}
      </SectionCard>

      <SectionCard
        title="CSV"
        toolbar={
          <Button size="sm" onClick={() => setShowCsv((s) => !s)}>
            {showCsv ? "Hide CSV" : "Load CSV"}
          </Button>
        }
      >
        {!showCsv ? (
          <StateEmpty message="Click 'Load CSV' to fetch the report CSV." />
        ) : csvQuery.isLoading ? (
          <Loading label="Loading CSV" />
        ) : csvQuery.isError ? (
          <Banner tone="danger" title="Could not load CSV">
            {(csvQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : csvQuery.data ? (
          <CsvPreview
            csv={csvQuery.data}
            filename={`${report.title || report.id}.csv`}
            downloadable
          />
        ) : null}
      </SectionCard>

      <SectionCard title="Actions">
        <div className="flex flex-wrap items-center gap-2">
          {allowed.map((action) => {
            const gate = getActionGateState(role, action);
            const disabled = gate !== "enabled";
            const isPending = pendingAction === action;
            const tone =
              action === "delete" || action === "delete_with_files"
                ? "danger"
                : "neutral";
            if (action === "update") return null;
            return (
              <Button
                key={action}
                size="sm"
                tone={tone}
                disabled={disabled || isPending}
                state={isPending ? "pending" : "idle"}
                title={disabled ? permissionHint(action) : undefined}
                onClick={() => {
                  switch (action) {
                    case "archive":
                      return archiveMutation.mutate();
                    case "restore":
                      return restoreMutation.mutate();
                    case "delete":
                      return setConfirmDeleteRecordOpen(true);
                    case "delete_with_files":
                      return setConfirmDeleteFilesOpen(true);
                    default:
                      return undefined;
                  }
                }}
              >
                {ACTION_LABELS[action]}
              </Button>
            );
          })}
          <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
            <InfoHint
              label="Notes and tags"
              text="Notes and tags require backend schema support that is not yet implemented."
            />
            Notes/tags: unsupported
          </span>
        </div>
      </SectionCard>

      <ConfirmDialog
        open={confirmDeleteRecordOpen}
        title="Delete report record"
        description="Type the report ID to confirm. Removes the report metadata. JSON and CSV files on disk are not touched."
        confirmLabel="Delete record"
        phrase={report.id}
        destructive
        pending={deleteRecordMutation.isPending}
        errorMessage={
          deleteRecordMutation.isError
            ? (deleteRecordMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteRecordMutation.mutate()}
        onClose={() => setConfirmDeleteRecordOpen(false)}
      />
      <ConfirmDialog
        open={confirmDeleteFilesOpen}
        title="Delete report and files"
        description="Type the report ID to confirm. Removes the metadata record AND the JSON/CSV files from disk."
        confirmLabel="Delete report and files"
        phrase={report.id}
        destructive
        pending={deleteFilesMutation.isPending}
        errorMessage={
          deleteFilesMutation.isError
            ? (deleteFilesMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteFilesMutation.mutate()}
        onClose={() => setConfirmDeleteFilesOpen(false)}
      />
    </div>
  );
}

function MetricLineChart({
  title,
  data,
  dataKey,
  colors,
  zeroCeiling = false,
}: {
  title: string;
  data: unknown[];
  dataKey: string;
  colors: ReturnType<typeof getChartColors>;
  zeroCeiling?: boolean;
}) {
  const valueDomain = chartValueDomain(data, dataKey, { zeroCeiling });
  return (
    <div>
      <h3 className="text-[12px] font-semibold uppercase tracking-wide text-muted">
        {title}
      </h3>
      <div className="mt-2 min-w-0">
        <ResponsiveContainer width="100%" height={220} minWidth={280}>
          <LineChart data={data} margin={{ top: 4, right: 8, bottom: 4, left: 0 }}>
            <CartesianGrid stroke={colors.gridColor} strokeDasharray="3 3" />
            <XAxis
              dataKey="ts"
              type="number"
              domain={["dataMin", "dataMax"]}
              tick={{ fill: colors.textColor, fontSize: 10 }}
              tickFormatter={formatChartTick}
            />
            <YAxis
              dataKey={dataKey}
              domain={valueDomain}
              tick={{ fill: colors.textColor, fontSize: 10 }}
              tickFormatter={formatChartAxisValue}
              width={72}
            />
            <Tooltip
              contentStyle={{
                background: colors.tooltipBg,
                border: `1px solid ${colors.borderColor}`,
                color: colors.textColor,
                fontSize: 11,
              }}
              formatter={(value: unknown, name: unknown) => [
                typeof value === "number"
                  ? formatChartAxisValue(value)
                  : String(value),
                typeof name === "string" ? humanize(name) : String(name),
              ]}
            />
            <Line
              type="monotone"
              dataKey={dataKey}
              stroke={colors.lineColor}
              strokeWidth={1.5}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function chartValueDomain(
  data: unknown[],
  dataKey: string,
  options: { zeroCeiling?: boolean } = {},
): [number, number] | undefined {
  const values = data.flatMap((row) => {
    if (typeof row !== "object" || row === null) return [];
    const value = (row as Record<string, unknown>)[dataKey];
    return typeof value === "number" && Number.isFinite(value) ? [value] : [];
  });
  if (values.length === 0) return undefined;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = Math.max(max - min, Math.abs(max) * 0.02, 1);
  const padding = span * 0.1;
  if (options.zeroCeiling) {
    return [min < 0 ? min - padding : -padding, 0];
  }
  return [min - padding, max + padding];
}

const SWEEP_RATE_COLUMNS = new Set([
  "win_rate",
  "fill_rate",
  "partial_fill_rate",
  "max_dd",
]);

const SWEEP_MONEY_COLUMNS = new Set([
  "pnl",
  "pnl_net",
  "gross_pnl",
  "total_fees",
  "final_balance",
  "hwm",
  "calibrated_pnl",
  "calibrated_final_balance",
  "baseline_replay_delta_pnl",
  "source_baseline_pnl",
  "baseline_replay_pnl",
]);

function formatSweepMetric(column: string, value: unknown): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return String(value ?? "");
  }
  if (SWEEP_RATE_COLUMNS.has(column)) {
    return `${(value * 100).toFixed(1)}%`;
  }
  if (SWEEP_MONEY_COLUMNS.has(column)) {
    return formatChartAxisValue(value);
  }
  if (Number.isInteger(value)) {
    return value.toString();
  }
  return value.toFixed(3);
}

function formatChartAxisValue(value: number): string {
  if (!Number.isFinite(value)) return "";
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  if (abs >= 1000) return `${sign}$${Math.round(abs).toLocaleString()}`;
  if (abs >= 100) return `${sign}$${abs.toFixed(0)}`;
  if (abs >= 10) return `${sign}$${abs.toFixed(1)}`;
  return `${sign}$${abs.toFixed(2)}`;
}

function formatSignedInteger(value: number | null | undefined): string {
  if (typeof value !== "number") return "n/a";
  return value > 0 ? `+${value.toLocaleString()}` : value.toLocaleString();
}
