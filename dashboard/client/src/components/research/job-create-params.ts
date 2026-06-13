import type {
  JobType,
  KeyValueRow,
  ResearchArtifact,
} from "../../lib/research-types";

export const EXPORT_RUN_MODES = [
  "paper",
  "live_readonly",
  "live_trading",
] as const;
export const EXPORT_SOURCE_STATES = ["stopped", "running_readonly"] as const;
export const LARGE_INTERVAL_MS = 6 * 60 * 60 * 1000;
export const DEFAULT_STARTING_BALANCE = "100";
export const DEFAULT_SWEEP_ROWS: KeyValueRow[] = [
  { key: "LATENCY_ARB_MIN_ASK", value: "0.25,0.30,0.35,0.40" },
  { key: "LATENCY_ARB_MAX_ASK", value: "0.60,0.70,0.80,0.90" },
  { key: "SIM_ORDER_LATENCY_MS", value: "100,250,500" },
];

export const PARAMETER_OPTIONS = [
  {
    key: "LATENCY_ARB_MIN_ASK",
    label: "Latency arb minimum ask",
    hint: "Minimum ask threshold for latency arbitrage entries.",
    placeholder: "0.30",
  },
  {
    key: "LATENCY_ARB_MAX_ASK",
    label: "Latency arb maximum ask",
    hint: "Maximum ask threshold accepted by latency arbitrage.",
    placeholder: "0.80",
  },
  {
    key: "LATENCY_ARB_COOLDOWN_MS",
    label: "Latency arb cooldown",
    hint: "Milliseconds to wait before another latency arbitrage entry.",
    placeholder: "30000",
  },
  {
    key: "LATENCY_ARB_MAX_POSITION_FRACTION",
    label: "Latency arb position fraction",
    hint: "Fraction of available balance used by latency arbitrage.",
    placeholder: "0.05",
  },
  {
    key: "SPREAD_CAPTURE_THRESHOLD",
    label: "Spread capture threshold",
    hint: "Threshold used by spread capture entries.",
    placeholder: "0.95",
  },
  {
    key: "SIM_ORDER_LATENCY_MS",
    label: "Simulated order latency",
    hint: "Milliseconds of simulated order latency in replay.",
    placeholder: "250",
  },
  {
    key: "TAKER_FEE_RATE",
    label: "Taker fee rate",
    hint: "Fee rate charged in simulated fills.",
    placeholder: "0.02",
  },
  {
    key: "MAX_BOOK_STALENESS_MS",
    label: "Max book staleness",
    hint: "Maximum accepted order book age in milliseconds.",
    placeholder: "500",
  },
] as const;

export type ParameterSource = "worker_defaults" | "custom";
export type SweepScope = "full" | "focused";
export type IntervalMode = "artifact" | "custom";
export type ExportRunMode = (typeof EXPORT_RUN_MODES)[number];
export type ExportSourceState = (typeof EXPORT_SOURCE_STATES)[number];

export interface ExportState {
  source_db_path: string;
  run_mode: ExportRunMode;
  source_state: ExportSourceState;
  interval_start_iso: string;
  interval_end_iso: string;
  log_paths: string;
  dry_run: boolean;
  confirm_export: boolean;
}

export interface BacktestState {
  artifact_id: string;
  data_db_path: string;
  interval_mode: IntervalMode;
  start_iso: string;
  end_iso: string;
  balance: string;
  param_source: ParameterSource;
  confirm_interval: boolean;
  setOverrides: KeyValueRow[];
}

export interface SweepState extends BacktestState {
  sweep_scope: SweepScope;
  sweeps: KeyValueRow[];
}

export type ExportInitialValues = Partial<ExportState>;

export type BacktestInitialValues = Partial<BacktestState>;

export type SweepInitialValues = Partial<SweepState>;

export interface JobCreateFormInitialValues {
  type?: JobType;
  priority?: number;
  export?: ExportInitialValues;
  backtest?: BacktestInitialValues;
  sweep?: SweepInitialValues;
  additionalParamsJson?: string;
}

export function isoToMs(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return undefined;
  return parsed;
}

export function datetimeLocalFromMs(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "";
  const date = new Date(value);
  const pad = (part: number) => part.toString().padStart(2, "0");
  return [
    date.getFullYear(),
    "-",
    pad(date.getMonth() + 1),
    "-",
    pad(date.getDate()),
    "T",
    pad(date.getHours()),
    ":",
    pad(date.getMinutes()),
  ].join("");
}

export type IntervalSource =
  | "explicit input"
  | "artifact fallback"
  | "missing"
  | "invalid";

export interface IntervalBoundary {
  ms: number | undefined;
  source: IntervalSource;
}

export interface EffectiveInterval {
  start: IntervalBoundary;
  end: IntervalBoundary;
  durationMs: number | undefined;
  valid: boolean;
  reason: string | null;
  requiresConfirmation: boolean;
}

export function resolveIntervalBoundary(
  value: string,
  artifactMs: number | null | undefined,
): IntervalBoundary {
  if (value.trim()) {
    const ms = isoToMs(value);
    return ms == null
      ? { ms: undefined, source: "invalid" }
      : { ms, source: "explicit input" };
  }
  return artifactMs == null
    ? { ms: undefined, source: "missing" }
    : { ms: artifactMs, source: "artifact fallback" };
}

export function effectiveInterval(
  state: BacktestState,
  artifact: ResearchArtifact | undefined,
): EffectiveInterval {
  const start =
    state.interval_mode === "artifact"
      ? resolveIntervalBoundary("", artifact?.interval_start_ms)
      : resolveIntervalBoundary(state.start_iso, null);
  const end =
    state.interval_mode === "artifact"
      ? resolveIntervalBoundary("", artifact?.interval_end_ms)
      : resolveIntervalBoundary(state.end_iso, null);
  let reason: string | null = null;
  let durationMs: number | undefined;
  if (start.source === "invalid" || end.source === "invalid") {
    reason = "Start and end must be valid datetimes.";
  } else if (start.ms == null || end.ms == null) {
    reason =
      state.interval_mode === "artifact"
        ? "The selected artifact does not include a usable interval."
        : "Custom start and end are required.";
  } else if (end.ms <= start.ms) {
    reason = "End must be after start.";
  } else {
    durationMs = end.ms - start.ms;
  }
  return {
    start,
    end,
    durationMs,
    valid: reason == null,
    reason,
    requiresConfirmation: durationMs != null && durationMs > LARGE_INTERVAL_MS,
  };
}

export function buildExportParams(state: ExportState): Record<string, unknown> {
  const params: Record<string, unknown> = {
    source_db_path: state.source_db_path.trim(),
    run_mode: state.run_mode,
    source_state: state.source_state,
    dry_run: state.dry_run,
  };
  if (!state.dry_run) {
    params.confirm_export = state.confirm_export;
  }
  const startMs = isoToMs(state.interval_start_iso);
  const endMs = isoToMs(state.interval_end_iso);
  if (startMs != null) params.interval_start_ms = startMs;
  if (endMs != null) params.interval_end_ms = endMs;
  const logs = state.log_paths
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (logs.length > 0) params.log_paths = logs;
  return params;
}

export function buildBacktestParams(
  state: BacktestState,
  artifact: ResearchArtifact | undefined,
): Record<string, unknown> {
  const params: Record<string, unknown> = {
    param_source: state.param_source,
    interval_mode: state.interval_mode,
  };
  if (state.data_db_path.trim()) {
    params.data_db_path = state.data_db_path.trim();
  }
  const interval = effectiveInterval(state, artifact);
  const start = interval.start.ms;
  const end = interval.end.ms;
  if (start != null) params.start_ms = start;
  if (end != null) params.end_ms = end;
  const balance = Number(state.balance);
  if (Number.isFinite(balance) && balance > 0) params.balance = balance;
  const sets = (state.param_source === "custom" ? state.setOverrides : [])
    .map((row) => ({
      key: row.key.trim(),
      value: row.value.trim(),
    }))
    .filter((row) => row.key && row.value)
    .map((row) => `${row.key}=${row.value}`);
  if (sets.length > 0) params.set = sets;
  return params;
}

export function parseAdditionalParams(value: string): {
  params: Record<string, unknown>;
  error: string | null;
} {
  const trimmed = value.trim();
  if (!trimmed) {
    return { params: {}, error: null };
  }
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {
        params: {},
        error: "Additional params must be a JSON object.",
      };
    }
    return { params: parsed as Record<string, unknown>, error: null };
  } catch (err) {
    return {
      params: {},
      error: err instanceof Error ? err.message : "Invalid JSON.",
    };
  }
}

export function mergeParams(
  additional: Record<string, unknown>,
  known: Record<string, unknown>,
): Record<string, unknown> {
  return {
    ...additional,
    ...known,
  };
}

export function jobTypeName(type: JobType): string {
  if (type === "export") return "Export";
  if (type === "sweep") return "Sweep";
  return "Backtest";
}

export function intervalSourceLabel(source: IntervalSource): string {
  if (source === "explicit input") return "Set by you";
  if (source === "artifact fallback") return "Artifact";
  if (source === "invalid") return "Invalid input";
  return "Missing";
}

export function rowsWithValues(rows: KeyValueRow[]): KeyValueRow[] {
  return rows.filter((row) => row.key.trim() && row.value.trim());
}

export function sweepRowsForState(state: SweepState): KeyValueRow[] {
  return state.sweep_scope === "full"
    ? DEFAULT_SWEEP_ROWS
    : rowsWithValues(state.sweeps);
}

export function sweepCombinationCount(rows: KeyValueRow[]): number {
  if (rows.length === 0) return 0;
  return rows.reduce((total, row) => {
    const values = row.value
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    return total * Math.max(values.length, 1);
  }, 1);
}

export function parameterOption(key: string) {
  return PARAMETER_OPTIONS.find((option) => option.key === key);
}

export function defaultParameterRows(rows: KeyValueRow[]): KeyValueRow[] {
  return rows.length > 0 ? rows : [{ key: "LATENCY_ARB_MIN_ASK", value: "" }];
}

export function initialIntervalMode(
  initial: BacktestInitialValues | SweepInitialValues | undefined,
): IntervalMode {
  if (
    initial?.interval_mode === "artifact" ||
    initial?.interval_mode === "custom"
  ) {
    return initial.interval_mode;
  }
  return initial?.start_iso || initial?.end_iso ? "custom" : "artifact";
}

export function artifactIntervalState<T extends BacktestState>(state: T): T {
  return {
    ...state,
    interval_mode: "artifact",
    start_iso: "",
    end_iso: "",
    confirm_interval: false,
  };
}

export function customIntervalState<T extends BacktestState>(
  state: T,
  artifact: ResearchArtifact | undefined,
): T {
  return {
    ...state,
    interval_mode: "custom",
    start_iso:
      state.start_iso || datetimeLocalFromMs(artifact?.interval_start_ms),
    end_iso: state.end_iso || datetimeLocalFromMs(artifact?.interval_end_ms),
    confirm_interval: false,
  };
}

export function artifactRecency(artifact: ResearchArtifact): number {
  return (
    artifact.interval_end_ms ??
    artifact.interval_start_ms ??
    artifact.updated_at ??
    artifact.created_at ??
    0
  );
}
