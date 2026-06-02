import type { ResearchReport } from "./research-types";

export interface ReportProvenance {
  job_id?: string;
  job_type?: string;
  artifact_id?: string | null;
  start?: string;
  end?: string;
  start_ms?: number | null;
  end_ms?: number | null;
  balance?: number;
  sets?: string[];
  sweeps?: string[];
  dashboard_image_ref?: string | null;
  research_worker_image_ref?: string | null;
}

export interface ReportMetrics {
  net_pnl?: number | null;
  gross_pnl?: number | null;
  total_fees?: number | null;
  final_balance?: number | null;
  trade_count?: number | null;
  wins?: number | null;
  losses?: number | null;
  win_rate?: number | null;
  max_drawdown?: number | null;
  max_drawdown_pct?: number | null;
  signal_count?: number | null;
  fill_count?: number | null;
  no_fill_count?: number | null;
}

export interface SourceRunMetrics {
  net_pnl?: number | null;
  gross_pnl?: number | null;
  total_fees?: number | null;
  final_balance?: number | null;
  trade_count?: number | null;
  signal_count?: number | null;
}

export interface SourceRunMetricDelta {
  net_pnl?: number | null;
  final_balance?: number | null;
  trade_count?: number | null;
  signal_count?: number | null;
}

export interface SourceRunComparison {
  status: string;
  source: SourceRunMetrics;
  replay: SourceRunMetrics;
  delta: SourceRunMetricDelta;
}

export interface EquityPoint {
  ts: number;
  equity: number;
}

export interface DrawdownPoint {
  ts: number;
  equity: number;
  high_water_mark: number;
  drawdown: number;
  drawdown_pct: number;
}

export interface RejectionReason {
  reason: string;
  count: number;
}

export interface SweepRow {
  rank?: number | null;
  params: Record<string, unknown>;
  metrics: Record<string, unknown>;
}

export interface SweepAnalysis {
  columns: string[];
  parameter_columns: string[];
  metric_columns: string[];
  ranked_by: string;
  rows: SweepRow[];
  top_rows: SweepRow[];
}

export interface SweepSummary {
  row_count: number;
  parameter_columns: string[];
  metric_columns: string[];
  ranked_by: string;
  top_row?: SweepRow | null;
}

export interface ParsedReportSummary {
  schema_version: number | null;
  has_analysis: boolean;
  provenance: ReportProvenance;
  metrics: ReportMetrics;
  source_comparison: SourceRunComparison | null;
  diagnostics: string[];
  sweep_summary: SweepSummary | null;
}

export interface ParsedReportPayload extends ParsedReportSummary {
  equity_curve: EquityPoint[];
  drawdown_curve: DrawdownPoint[];
  rejection_reasons: RejectionReason[];
  sweep: SweepAnalysis | null;
}

export type ReportSortKey =
  | "net_pnl_desc"
  | "updated_desc"
  | "drawdown_best"
  | "win_rate_desc"
  | "trades_desc";

export function parseReportSummary(report: ResearchReport): ParsedReportSummary {
  const parsed = parseJsonObject(report.summary_json);
  return summaryFromObject(parsed, report);
}

export function parseReportPayload(
  payload: unknown,
  report?: ResearchReport,
): ParsedReportPayload {
  const obj = asRecord(payload);
  const summary = summaryFromObject(obj, report);
  return {
    ...summary,
    equity_curve: readArray(obj?.equity_curve).flatMap(readEquityPoint),
    drawdown_curve: readArray(obj?.drawdown_curve).flatMap(readDrawdownPoint),
    rejection_reasons: readArray(obj?.rejection_reasons).flatMap(
      readRejectionReason,
    ),
    sweep: readSweep(obj?.sweep),
  };
}

export function sortReports(
  reports: ResearchReport[],
  sortKey: ReportSortKey,
): ResearchReport[] {
  return [...reports].sort((a, b) => compareReports(a, b, sortKey));
}

export function compareReports(
  a: ResearchReport,
  b: ResearchReport,
  sortKey: ReportSortKey,
): number {
  const pa = parseReportSummary(a);
  const pb = parseReportSummary(b);
  const fallback = b.updated_at - a.updated_at;
  if (sortKey === "updated_desc") return fallback;
  if (sortKey === "drawdown_best") {
    return (
      optionalMetric(pb.metrics.max_drawdown) -
        optionalMetric(pa.metrics.max_drawdown) || fallback
    );
  }
  if (sortKey === "win_rate_desc") {
    return metricDesc(pa.metrics.win_rate, pb.metrics.win_rate) || fallback;
  }
  if (sortKey === "trades_desc") {
    return metricDesc(pa.metrics.trade_count, pb.metrics.trade_count) || fallback;
  }
  return metricDesc(pa.metrics.net_pnl, pb.metrics.net_pnl) || fallback;
}

export function comparisonWarnings(
  reports: Array<{ report: ResearchReport; parsed: ParsedReportPayload }>,
): string[] {
  const warnings: string[] = [];
  if (distinct(reports.map((r) => r.parsed.provenance.artifact_id)).length > 1) {
    warnings.push("Reports use different artifacts.");
  }
  if (distinct(reports.map((r) => r.parsed.provenance.job_type)).length > 1) {
    warnings.push("Reports use different job types.");
  }
  if (distinct(reports.map((r) => r.parsed.provenance.start_ms)).length > 1) {
    warnings.push("Reports use different start times.");
  }
  if (distinct(reports.map((r) => r.parsed.provenance.end_ms)).length > 1) {
    warnings.push("Reports use different end times.");
  }
  if (distinct(reports.map((r) => r.parsed.provenance.balance)).length > 1) {
    warnings.push("Reports use different starting balances.");
  }
  if (reports.some((r) => reportHasSourceMismatch(r.parsed))) {
    warnings.push("One or more reports differ from source-run metrics.");
  }
  if (reports.some((r) => reportUsesCalibratedSweep(r.parsed))) {
    warnings.push("One or more sweep reports are ranked by calibrated PnL.");
  }
  return warnings;
}

export function bestReportLabel(
  reports: Array<{ report: ResearchReport; parsed: ParsedReportPayload }>,
): string {
  const scored = reports.filter((r) => numberOrNull(r.parsed.metrics.net_pnl) != null);
  const label = reports.some((r) => reportUsesCalibratedSweep(r.parsed))
    ? "Calibrated Net PnL"
    : reports.some((r) => reportHasSourceComparison(r.parsed))
    ? "Replay Net PnL"
    : "Net PnL";
  if (scored.length === 0) return `No winner: ${label} is unavailable.`;
  const sorted = [...scored].sort(
    (a, b) =>
      optionalMetric(b.parsed.metrics.net_pnl) -
      optionalMetric(a.parsed.metrics.net_pnl),
  );
  const first = sorted[0];
  const second = sorted[1];
  if (
    first &&
    second &&
    optionalMetric(first.parsed.metrics.net_pnl) ===
      optionalMetric(second.parsed.metrics.net_pnl)
  ) {
    return `No winner: top ${label} is tied.`;
  }
  return first ? `Best by ${label}: ${first.report.title}` : "No winner.";
}

export function reportHasSourceComparison(
  parsed: ParsedReportSummary | ParsedReportPayload,
): boolean {
  return parsed.source_comparison != null;
}

export function reportHasSourceMismatch(
  parsed: ParsedReportSummary | ParsedReportPayload,
): boolean {
  return parsed.source_comparison?.status === "mismatch";
}

export function netPnlMetricLabel(
  parsed: ParsedReportSummary | ParsedReportPayload,
): string {
  if (reportUsesCalibratedSweep(parsed)) {
    return "Calibrated Net PnL";
  }
  return reportHasSourceComparison(parsed) ? "Replay Net PnL" : "Net PnL";
}

export function reportUsesCalibratedSweep(
  parsed: ParsedReportSummary | ParsedReportPayload,
): boolean {
  const payloadSweep = "sweep" in parsed ? parsed.sweep : null;
  return (
    parsed.sweep_summary?.ranked_by === "calibrated_pnl" ||
    payloadSweep?.ranked_by === "calibrated_pnl"
  );
}

function summaryFromObject(
  obj: Record<string, unknown> | null,
  report?: ResearchReport,
): ParsedReportSummary {
  const provenance = readProvenance(obj?.provenance, report);
  const metrics = readMetrics(obj?.metrics ?? obj);
  return {
    schema_version: numberOrNull(obj?.schema_version),
    has_analysis: numberOrNull(obj?.schema_version) === 2,
    provenance,
    metrics,
    source_comparison: readSourceComparison(obj?.source_comparison),
    diagnostics: readArray(obj?.diagnostics).flatMap((value) =>
      typeof value === "string" ? [value] : [],
    ),
    sweep_summary: readSweepSummary(obj?.sweep_summary),
  };
}

function readProvenance(
  value: unknown,
  report?: ResearchReport,
): ReportProvenance {
  const obj = asRecord(value);
  return {
    job_id: stringOrUndefined(obj?.job_id) ?? report?.job_id,
    job_type: stringOrUndefined(obj?.job_type),
    artifact_id:
      stringOrUndefined(obj?.artifact_id) ?? report?.artifact_id ?? null,
    start: stringOrUndefined(obj?.start),
    end: stringOrUndefined(obj?.end),
    start_ms: numberOrNull(obj?.start_ms),
    end_ms: numberOrNull(obj?.end_ms),
    balance: numberOrUndefined(obj?.balance),
    sets: readStringArray(obj?.sets),
    sweeps: readStringArray(obj?.sweeps),
    dashboard_image_ref: stringOrUndefined(obj?.dashboard_image_ref) ?? null,
    research_worker_image_ref:
      stringOrUndefined(obj?.research_worker_image_ref) ?? null,
  };
}

function readMetrics(value: unknown): ReportMetrics {
  const obj = asRecord(value);
  return {
    net_pnl: numberOrNull(obj?.net_pnl),
    gross_pnl: numberOrNull(obj?.gross_pnl),
    total_fees: numberOrNull(obj?.total_fees),
    final_balance: numberOrNull(obj?.final_balance),
    trade_count: numberOrNull(obj?.trade_count),
    wins: numberOrNull(obj?.wins),
    losses: numberOrNull(obj?.losses),
    win_rate: numberOrNull(obj?.win_rate),
    max_drawdown: numberOrNull(obj?.max_drawdown),
    max_drawdown_pct: numberOrNull(obj?.max_drawdown_pct),
    signal_count: numberOrNull(obj?.signal_count),
    fill_count: numberOrNull(obj?.fill_count),
    no_fill_count: numberOrNull(obj?.no_fill_count),
  };
}

function readSourceComparison(value: unknown): SourceRunComparison | null {
  const obj = asRecord(value);
  if (!obj) return null;
  const source = readSourceRunMetrics(obj.source);
  const replay = readSourceRunMetrics(obj.replay);
  const delta = readSourceRunDelta(obj.delta);
  return {
    status: stringOrUndefined(obj.status) ?? "unknown",
    source,
    replay,
    delta,
  };
}

function readSourceRunMetrics(value: unknown): SourceRunMetrics {
  const obj = asRecord(value);
  return {
    net_pnl: numberOrNull(obj?.net_pnl),
    gross_pnl: numberOrNull(obj?.gross_pnl),
    total_fees: numberOrNull(obj?.total_fees),
    final_balance: numberOrNull(obj?.final_balance),
    trade_count: numberOrNull(obj?.trade_count),
    signal_count: numberOrNull(obj?.signal_count),
  };
}

function readSourceRunDelta(value: unknown): SourceRunMetricDelta {
  const obj = asRecord(value);
  return {
    net_pnl: numberOrNull(obj?.net_pnl),
    final_balance: numberOrNull(obj?.final_balance),
    trade_count: numberOrNull(obj?.trade_count),
    signal_count: numberOrNull(obj?.signal_count),
  };
}

function readEquityPoint(value: unknown): EquityPoint[] {
  const obj = asRecord(value);
  const ts = numberOrNull(obj?.ts);
  const equity = numberOrNull(obj?.equity);
  return ts == null || equity == null ? [] : [{ ts, equity }];
}

function readDrawdownPoint(value: unknown): DrawdownPoint[] {
  const obj = asRecord(value);
  const ts = numberOrNull(obj?.ts);
  const equity = numberOrNull(obj?.equity);
  const high_water_mark = numberOrNull(obj?.high_water_mark);
  const drawdown = numberOrNull(obj?.drawdown);
  const drawdown_pct = numberOrNull(obj?.drawdown_pct);
  if (
    ts == null ||
    equity == null ||
    high_water_mark == null ||
    drawdown == null ||
    drawdown_pct == null
  ) {
    return [];
  }
  return [{ ts, equity, high_water_mark, drawdown, drawdown_pct }];
}

function readRejectionReason(value: unknown): RejectionReason[] {
  const obj = asRecord(value);
  const reason = stringOrUndefined(obj?.reason);
  const count = numberOrNull(obj?.count);
  return reason && count != null ? [{ reason, count }] : [];
}

function readSweep(value: unknown): SweepAnalysis | null {
  const obj = asRecord(value);
  if (!obj) return null;
  return {
    columns: readStringArray(obj.columns),
    parameter_columns: readStringArray(obj.parameter_columns),
    metric_columns: readStringArray(obj.metric_columns),
    ranked_by: stringOrUndefined(obj.ranked_by) ?? "pnl",
    rows: readArray(obj.rows).flatMap(readSweepRow),
    top_rows: readArray(obj.top_rows).flatMap(readSweepRow),
  };
}

function readSweepSummary(value: unknown): SweepSummary | null {
  const obj = asRecord(value);
  if (!obj) return null;
  return {
    row_count: numberOrNull(obj.row_count) ?? 0,
    parameter_columns: readStringArray(obj.parameter_columns),
    metric_columns: readStringArray(obj.metric_columns),
    ranked_by: stringOrUndefined(obj.ranked_by) ?? "pnl",
    top_row: readSweepRow(obj.top_row)[0] ?? null,
  };
}

function readSweepRow(value: unknown): SweepRow[] {
  const obj = asRecord(value);
  if (!obj) return [];
  return [
    {
      rank: numberOrNull(obj.rank),
      params: asRecord(obj.params) ?? {},
      metrics: asRecord(obj.metrics) ?? {},
    },
  ];
}

function parseJsonObject(value: string | null): Record<string, unknown> | null {
  if (!value) return null;
  try {
    return asRecord(JSON.parse(value));
  } catch {
    return null;
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readStringArray(value: unknown): string[] {
  return readArray(value).flatMap((entry) =>
    typeof entry === "string" ? [entry] : [],
  );
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function numberOrUndefined(value: unknown): number | undefined {
  return numberOrNull(value) ?? undefined;
}

function optionalMetric(value: number | null | undefined): number {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : Number.NEGATIVE_INFINITY;
}

function metricDesc(
  a: number | null | undefined,
  b: number | null | undefined,
): number {
  return optionalMetric(b) - optionalMetric(a);
}

function distinct(values: unknown[]): string[] {
  return Array.from(new Set(values.map((value) => JSON.stringify(value))));
}
