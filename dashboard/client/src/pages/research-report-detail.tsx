import { useMemo, useState } from "react";
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
import type {
  ResearchAction,
  UpdateReportRequest,
} from "../lib/research-types";
import { formatDateTime, formatSignedUsd, humanize } from "../lib/utils";

interface EquityPoint {
  ts: number;
  equity: number;
}

interface SummaryShape {
  net_pnl?: number;
  max_drawdown?: number;
  win_rate?: number;
  trade_count?: number;
}

function parseSummary(value: string | null): SummaryShape | null {
  if (!value) return null;
  try {
    return JSON.parse(value) as SummaryShape;
  } catch {
    return null;
  }
}

function extractEquityCurve(payload: unknown): EquityPoint[] {
  if (typeof payload !== "object" || payload === null) return [];
  const record = payload as { equity_curve?: unknown };
  const arr = record.equity_curve;
  if (!Array.isArray(arr)) return [];
  return arr
    .map((entry): EquityPoint | null => {
      if (typeof entry !== "object" || entry === null) return null;
      const e = entry as { ts?: unknown; equity?: unknown };
      const ts = typeof e.ts === "number" ? e.ts : null;
      const equity = typeof e.equity === "number" ? e.equity : null;
      if (ts == null || equity == null) return null;
      return { ts, equity };
    })
    .filter((point): point is EquityPoint => point !== null);
}

function extractSweepPoints(
  payload: unknown,
): Array<Record<string, unknown>> | null {
  if (typeof payload !== "object" || payload === null) return null;
  const record = payload as { sweep_points?: unknown };
  if (!Array.isArray(record.sweep_points)) return null;
  return record.sweep_points.filter(
    (row): row is Record<string, unknown> =>
      typeof row === "object" && row !== null,
  );
}

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
  const jsonQuery = useResearchReportJson(id, showJson);
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

  const summary = useMemo(
    () => parseSummary(reportQuery.data?.summary_json ?? null),
    [reportQuery.data],
  );
  const equity = useMemo(
    () => extractEquityCurve(jsonQuery.data),
    [jsonQuery.data],
  );
  const sweepPoints = useMemo(
    () => extractSweepPoints(jsonQuery.data),
    [jsonQuery.data],
  );

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
  const allowed = getAllowedActions("report", report.status);
  const canEditTitle = role ? canPerform(role, "update") : false;
  const filesAppearMissing =
    showJson && jsonQuery.isError && report.report_path != null;

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

      {summary && (
        <SectionCard title="Summary metrics">
          <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">
            {summary.net_pnl != null && (
              <MetricCard
                label="Net PnL"
                value={formatSignedUsd(summary.net_pnl)}
                tone={summary.net_pnl >= 0 ? "success" : "danger"}
              />
            )}
            {summary.max_drawdown != null && (
              <MetricCard
                label="Max drawdown"
                value={formatSignedUsd(-Math.abs(summary.max_drawdown))}
                tone="warning"
              />
            )}
            {summary.win_rate != null && (
              <MetricCard
                label="Win rate"
                value={`${(summary.win_rate * 100).toFixed(1)}%`}
              />
            )}
            {summary.trade_count != null && (
              <MetricCard
                label="Trades"
                value={summary.trade_count.toLocaleString()}
              />
            )}
          </div>
        </SectionCard>
      )}

      <SectionCard
        title="Report JSON"
        toolbar={
          <Button size="sm" onClick={() => setShowJson((s) => !s)}>
            {showJson ? "Hide" : "Load JSON"}
          </Button>
        }
      >
        {!showJson ? (
          <StateEmpty message="Click 'Load JSON' to fetch the report JSON. Equity and sweep visualizations will render after the load." />
        ) : jsonQuery.isLoading ? (
          <Loading label="Loading report JSON" />
        ) : jsonQuery.isError ? (
          <Banner tone="danger" title="Could not load report JSON">
            {(jsonQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : (
          <div className="space-y-3">
            {equity.length > 0 && (
              <div>
                <h3 className="text-[12px] font-semibold uppercase tracking-wide text-muted">
                  Equity curve
                </h3>
                <div style={{ width: "100%", height: 200 }} className="mt-2">
                  <ResponsiveContainer>
                    <LineChart
                      data={equity}
                      margin={{ top: 4, right: 8, bottom: 4, left: 0 }}
                    >
                      <CartesianGrid
                        stroke={colors.gridColor}
                        strokeDasharray="3 3"
                      />
                      <XAxis
                        dataKey="ts"
                        type="number"
                        domain={["dataMin", "dataMax"]}
                        tick={{ fill: colors.textColor, fontSize: 10 }}
                        tickFormatter={(value: number) =>
                          new Date(value).toLocaleTimeString()
                        }
                      />
                      <YAxis
                        tick={{ fill: colors.textColor, fontSize: 10 }}
                        width={56}
                      />
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
                        dataKey="equity"
                        stroke={colors.lineColor}
                        strokeWidth={1.5}
                        dot={false}
                        isAnimationActive={false}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              </div>
            )}
            {sweepPoints && sweepPoints.length > 0 && (
              <div>
                <h3 className="text-[12px] font-semibold uppercase tracking-wide text-muted">
                  Sweep points
                </h3>
                <div className="mt-2 overflow-x-auto border border-border">
                  <table className="w-full border-collapse text-[11px]">
                    <thead className="border-b border-border bg-surface text-left text-[10px] uppercase text-muted">
                      <tr>
                        {Object.keys(sweepPoints[0]).map((k) => (
                          <th key={k} className="px-2 py-1 font-semibold">
                            {k}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {sweepPoints.slice(0, 50).map((row, idx) => (
                        <tr
                          key={idx}
                          className="border-b border-border last:border-b-0"
                        >
                          {Object.keys(sweepPoints[0]).map((k) => (
                            <td key={k} className="px-2 py-1 tabular-nums">
                              {String(row[k] ?? "")}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
            <JsonViewer
              value={jsonQuery.data}
              label="Raw payload"
              maxHeight={300}
            />
          </div>
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
