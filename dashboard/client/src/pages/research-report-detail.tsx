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
  permissionHint,
  reportTone,
} from "../lib/research-permissions";
import {
  parseReportPayload,
  parseReportSummary,
} from "../lib/research-report-analysis";
import type {
  ResearchAction,
  UpdateReportRequest,
} from "../lib/research-types";
import { formatDateTime, formatSignedUsd, humanize } from "../lib/utils";

export function ResearchReportDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;
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
      navigate("/research/reports");
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const deleteFilesMutation = useMutation({
    mutationFn: () => deleteResearchReport(id, true),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
      navigate("/research/reports");
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
          to="/research/reports"
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
                "—"
              ),
            },
            {
              label: "Job type",
              value: analysis.provenance.job_type
                ? humanize(analysis.provenance.job_type)
                : "—",
            },
            {
              label: "Interval",
              value:
                analysis.provenance.start && analysis.provenance.end
                  ? `${analysis.provenance.start} → ${analysis.provenance.end}`
                  : "—",
            },
            {
              label: "Starting balance",
              value:
                analysis.provenance.balance != null
                  ? formatSignedUsd(analysis.provenance.balance).replace("+", "")
                  : "—",
            },
            {
              label: "Report path",
              value: report.report_path ? (
                <span className="font-mono text-[11px]">
                  {report.report_path}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: "CSV path",
              value: report.csv_path ? (
                <span className="font-mono text-[11px]">{report.csv_path}</span>
              ) : (
                "—"
              ),
            },
            {
              label: "Worker image",
              value: analysis.provenance.research_worker_image_ref ? (
                <span className="font-mono text-[11px]">
                  {analysis.provenance.research_worker_image_ref}
                </span>
              ) : (
                "—"
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
            label="Net PnL"
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
          <MetricCard
            label="Signals"
            value={formatInteger(metrics.signal_count)}
          />
          <MetricCard label="Wins" value={formatInteger(metrics.wins)} />
          <MetricCard label="Losses" value={formatInteger(metrics.losses)} />
        </div>
      </SectionCard>

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
            />
          </div>
        ) : (
          <StateEmpty message="No equity or drawdown series is available for this report." />
        )}
      </SectionCard>

      {sweep && (
        <SectionCard title="Sweep ranking">
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
                          {column}
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
                          {String(row.metrics[column] ?? "")}
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
            {showJson ? "Hide raw" : "Show raw"}
          </Button>
        }
      >
        {!showJson ? (
          <StateEmpty message="Raw JSON is hidden." />
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
            {showCsv ? "Hide" : "Load CSV"}
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
        description="Removes the report metadata. JSON and CSV files on disk are not touched."
        confirmLabel="Delete record"
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
}: {
  title: string;
  data: unknown[];
  dataKey: string;
  colors: ReturnType<typeof getChartColors>;
}) {
  return (
    <div>
      <h3 className="text-[12px] font-semibold uppercase tracking-wide text-muted">
        {title}
      </h3>
      <div style={{ width: "100%", height: 220 }} className="mt-2">
        <ResponsiveContainer>
          <LineChart data={data} margin={{ top: 4, right: 8, bottom: 4, left: 0 }}>
            <CartesianGrid stroke={colors.gridColor} strokeDasharray="3 3" />
            <XAxis
              dataKey="ts"
              type="number"
              domain={["dataMin", "dataMax"]}
              tick={{ fill: colors.textColor, fontSize: 10 }}
              tickFormatter={(value: number) => new Date(value).toLocaleTimeString()}
            />
            <YAxis tick={{ fill: colors.textColor, fontSize: 10 }} width={64} />
            <Tooltip
              contentStyle={{
                background: colors.tooltipBg,
                border: `1px solid ${colors.borderColor}`,
                color: colors.textColor,
                fontSize: 11,
              }}
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

function formatMetricUsd(value: number | null | undefined): string {
  return typeof value === "number" ? formatSignedUsd(value) : "n/a";
}

function formatUsd(value: number | null | undefined): string {
  return typeof value === "number"
    ? formatSignedUsd(value).replace("+", "")
    : "n/a";
}

function formatPercent(value: number | null | undefined): string {
  return typeof value === "number" ? `${(value * 100).toFixed(1)}%` : "n/a";
}

function formatInteger(value: number | null | undefined): string {
  return typeof value === "number" ? value.toLocaleString() : "n/a";
}
