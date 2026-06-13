import { useCallback, useMemo, useRef, useState } from "react";
import {
  Banner,
  Button,
  FormField,
  Input,
  Segment,
  StatusChip,
  Textarea,
} from "../ui/dashboard-primitives";
import type {
  CreateJobRequest,
  JobType,
  KeyValueRow,
  ResearchArtifact,
} from "../../lib/research-types";
import { formatBytes, formatDateTime, formatDurationShort } from "../../lib/utils";

const EXPORT_RUN_MODES = ["paper", "live_readonly", "live_trading"] as const;
const EXPORT_SOURCE_STATES = ["stopped", "running_readonly"] as const;
const LARGE_INTERVAL_MS = 6 * 60 * 60 * 1000;
const DEFAULT_STARTING_BALANCE = "100";
const DEFAULT_SWEEP_ROWS: KeyValueRow[] = [
  { key: "LATENCY_ARB_MIN_ASK", value: "0.25,0.30,0.35,0.40" },
  { key: "LATENCY_ARB_MAX_ASK", value: "0.60,0.70,0.80,0.90" },
  { key: "SIM_ORDER_LATENCY_MS", value: "100,250,500" },
];

const PARAMETER_OPTIONS = [
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

type ParameterSource = "worker_defaults" | "custom";
type SweepScope = "full" | "focused";
type IntervalMode = "artifact" | "custom";
type ExportRunMode = (typeof EXPORT_RUN_MODES)[number];
type ExportSourceState = (typeof EXPORT_SOURCE_STATES)[number];

interface ExportState {
  source_db_path: string;
  run_mode: ExportRunMode;
  source_state: ExportSourceState;
  interval_start_iso: string;
  interval_end_iso: string;
  log_paths: string;
  dry_run: boolean;
  confirm_export: boolean;
}

interface BacktestState {
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

interface SweepState extends BacktestState {
  sweep_scope: SweepScope;
  sweeps: KeyValueRow[];
}

type ExportInitialValues = Partial<ExportState>;

type BacktestInitialValues = Partial<BacktestState>;

type SweepInitialValues = Partial<SweepState>;

export interface JobCreateFormInitialValues {
  type?: JobType;
  priority?: number;
  export?: ExportInitialValues;
  backtest?: BacktestInitialValues;
  sweep?: SweepInitialValues;
  additionalParamsJson?: string;
}

interface JobCreateFormProps {
  artifacts: ResearchArtifact[];
  initialType?: JobType;
  initialValues?: JobCreateFormInitialValues;
  typeLocked?: boolean;
  showPriority?: boolean;
  showAdditionalParams?: boolean;
  submitLabel?: string;
  submitDisabled?: boolean;
  submitDisabledReason?: string;
  errorTitle?: string;
  pending: boolean;
  error: string | null;
  onSubmit: (req: CreateJobRequest) => void;
  onCancel?: () => void;
}

function isoToMs(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return undefined;
  return parsed;
}

function datetimeLocalFromMs(value: number | null | undefined): string {
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

type IntervalSource = "explicit input" | "artifact fallback" | "missing" | "invalid";

interface IntervalBoundary {
  ms: number | undefined;
  source: IntervalSource;
}

interface EffectiveInterval {
  start: IntervalBoundary;
  end: IntervalBoundary;
  durationMs: number | undefined;
  valid: boolean;
  reason: string | null;
  requiresConfirmation: boolean;
}

function resolveIntervalBoundary(
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

function effectiveInterval(
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
    requiresConfirmation:
      durationMs != null && durationMs > LARGE_INTERVAL_MS,
  };
}

function buildExportParams(state: ExportState): Record<string, unknown> {
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

function buildBacktestParams(
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

function parseAdditionalParams(value: string): {
  params: Record<string, unknown>;
  error: string | null;
} {
  const trimmed = value.trim();
  if (!trimmed) {
    return { params: {}, error: null };
  }
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (
      parsed == null ||
      typeof parsed !== "object" ||
      Array.isArray(parsed)
    ) {
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

function mergeParams(
  additional: Record<string, unknown>,
  known: Record<string, unknown>,
): Record<string, unknown> {
  return {
    ...additional,
    ...known,
  };
}

function jobTypeName(type: JobType): string {
  if (type === "export") return "Export";
  if (type === "sweep") return "Sweep";
  return "Backtest";
}

function intervalSourceLabel(source: IntervalSource): string {
  if (source === "explicit input") return "Set by you";
  if (source === "artifact fallback") return "Artifact";
  if (source === "invalid") return "Invalid input";
  return "Missing";
}

function rowsWithValues(rows: KeyValueRow[]): KeyValueRow[] {
  return rows.filter((row) => row.key.trim() && row.value.trim());
}

function sweepRowsForState(state: SweepState): KeyValueRow[] {
  return state.sweep_scope === "full"
    ? DEFAULT_SWEEP_ROWS
    : rowsWithValues(state.sweeps);
}

function sweepCombinationCount(rows: KeyValueRow[]): number {
  if (rows.length === 0) return 0;
  return rows.reduce((total, row) => {
    const values = row.value
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    return total * Math.max(values.length, 1);
  }, 1);
}

function parameterOption(key: string) {
  return PARAMETER_OPTIONS.find((option) => option.key === key);
}

function defaultParameterRows(rows: KeyValueRow[]): KeyValueRow[] {
  return rows.length > 0 ? rows : [{ key: "LATENCY_ARB_MIN_ASK", value: "" }];
}

function initialIntervalMode(
  initial: BacktestInitialValues | SweepInitialValues | undefined,
): IntervalMode {
  if (initial?.interval_mode === "artifact" || initial?.interval_mode === "custom") {
    return initial.interval_mode;
  }
  return initial?.start_iso || initial?.end_iso ? "custom" : "artifact";
}

function artifactIntervalState<T extends BacktestState>(state: T): T {
  return {
    ...state,
    interval_mode: "artifact",
    start_iso: "",
    end_iso: "",
    confirm_interval: false,
  };
}

function customIntervalState<T extends BacktestState>(
  state: T,
  artifact: ResearchArtifact | undefined,
): T {
  return {
    ...state,
    interval_mode: "custom",
    start_iso: state.start_iso || datetimeLocalFromMs(artifact?.interval_start_ms),
    end_iso: state.end_iso || datetimeLocalFromMs(artifact?.interval_end_ms),
    confirm_interval: false,
  };
}

function artifactRecency(artifact: ResearchArtifact): number {
  return (
    artifact.interval_end_ms ??
    artifact.interval_start_ms ??
    artifact.updated_at ??
    artifact.created_at ??
    0
  );
}

export function JobCreateForm({
  artifacts,
  initialType = "current_params",
  initialValues,
  typeLocked = false,
  showPriority = false,
  showAdditionalParams = false,
  submitLabel = "Create job",
  submitDisabled = false,
  submitDisabledReason,
  errorTitle = "Job creation failed",
  pending,
  error,
  onSubmit,
  onCancel,
}: JobCreateFormProps) {
  const availableArtifacts = useMemo(
    () => {
      const filtered = artifacts.filter(
        (artifact) => artifact.status === "available",
      );
      filtered.sort((left, right) => artifactRecency(right) - artifactRecency(left));
      return filtered;
    },
    [artifacts],
  );

  const startingType = initialValues?.type ?? initialType;
  const [type, setType] = useState<JobType>(startingType);
  const [priority, setPriority] = useState(
    String(initialValues?.priority ?? 0),
  );
  const [additionalParamsJson, setAdditionalParamsJson] = useState(
    initialValues?.additionalParamsJson ?? "",
  );

  const [exportState, setExportState] = useState<ExportState>({
    source_db_path: initialValues?.export?.source_db_path ?? "",
    run_mode: initialValues?.export?.run_mode ?? "live_readonly",
    source_state: initialValues?.export?.source_state ?? "stopped",
    interval_start_iso: initialValues?.export?.interval_start_iso ?? "",
    interval_end_iso: initialValues?.export?.interval_end_iso ?? "",
    log_paths: initialValues?.export?.log_paths ?? "",
    dry_run: initialValues?.export?.dry_run ?? true,
    confirm_export: initialValues?.export?.confirm_export ?? false,
  });

  const initialArtifactId = availableArtifacts[0]?.id ?? "";
  const initialBacktestOverrides = initialValues?.backtest?.setOverrides ?? [];
  const initialSweepOverrides = initialValues?.sweep?.setOverrides ?? [];
  const initialSweepRows = initialValues?.sweep?.sweeps ?? [];

  const [backtestState, setBacktestState] = useState<BacktestState>({
    artifact_id: initialValues?.backtest?.artifact_id ?? initialArtifactId,
    data_db_path: initialValues?.backtest?.data_db_path ?? "",
    interval_mode: initialIntervalMode(initialValues?.backtest),
    start_iso: initialValues?.backtest?.start_iso ?? "",
    end_iso: initialValues?.backtest?.end_iso ?? "",
    balance: initialValues?.backtest?.balance ?? DEFAULT_STARTING_BALANCE,
    param_source:
      initialValues?.backtest?.param_source ??
      (initialBacktestOverrides.length > 0 ? "custom" : "worker_defaults"),
    confirm_interval: initialValues?.backtest?.confirm_interval ?? false,
    setOverrides: defaultParameterRows(initialBacktestOverrides),
  });

  const [sweepState, setSweepState] = useState<SweepState>({
    artifact_id: initialValues?.sweep?.artifact_id ?? initialArtifactId,
    data_db_path: initialValues?.sweep?.data_db_path ?? "",
    interval_mode: initialIntervalMode(initialValues?.sweep),
    start_iso: initialValues?.sweep?.start_iso ?? "",
    end_iso: initialValues?.sweep?.end_iso ?? "",
    balance: initialValues?.sweep?.balance ?? DEFAULT_STARTING_BALANCE,
    param_source:
      initialValues?.sweep?.param_source ??
      (initialSweepOverrides.length > 0 ? "custom" : "worker_defaults"),
    confirm_interval: initialValues?.sweep?.confirm_interval ?? false,
    setOverrides: defaultParameterRows(initialSweepOverrides),
    sweep_scope: initialSweepRows.length > 0 ? "focused" : "full",
    sweeps: initialSweepRows.length > 0 ? initialSweepRows : DEFAULT_SWEEP_ROWS,
  });

  const additionalParams = useMemo(
    () => parseAdditionalParams(additionalParamsJson),
    [additionalParamsJson],
  );
  const priorityNumber = Number(priority);
  const priorityValid =
    !showPriority ||
    (Number.isInteger(priorityNumber) && priority.trim().length > 0);

  const exportBlocked = exportState.run_mode === "live_trading";
  const exportConfirmed = exportState.dry_run || exportState.confirm_export;
  const exportValid =
    !exportBlocked &&
    exportState.source_db_path.trim().length > 0 &&
    exportConfirmed;

  const backtestArtifact = availableArtifacts.find(
    (a) => a.id === backtestState.artifact_id,
  );
  const backtestInterval = effectiveInterval(backtestState, backtestArtifact);
  const backtestIntervalConfirmed =
    !backtestInterval.requiresConfirmation || backtestState.confirm_interval;
  const backtestValid =
    backtestState.artifact_id.trim().length > 0 &&
    backtestInterval.valid &&
    backtestIntervalConfirmed &&
    Number.isFinite(Number(backtestState.balance)) &&
    Number(backtestState.balance) > 0;

  const sweepArtifact = availableArtifacts.find(
    (a) => a.id === sweepState.artifact_id,
  );
  const sweepRows = sweepRowsForState(sweepState);
  const sweepInterval = effectiveInterval(sweepState, sweepArtifact);
  const sweepIntervalConfirmed =
    !sweepInterval.requiresConfirmation || sweepState.confirm_interval;
  const sweepValid =
    sweepState.artifact_id.trim().length > 0 &&
    sweepRows.length >= 1 &&
    sweepInterval.valid &&
    sweepIntervalConfirmed &&
    Number.isFinite(Number(sweepState.balance)) &&
    Number(sweepState.balance) > 0;

  const isValid =
    additionalParams.error == null &&
    priorityValid &&
    (type === "export"
      ? exportValid
      : type === "current_params"
        ? backtestValid
        : sweepValid);

  const submit = () => {
    if (!isValid || pending || submitDisabled) return;
    const priorityPayload = showPriority ? { priority: priorityNumber } : {};
    if (type === "export") {
      onSubmit({
        job_type: "export",
        ...priorityPayload,
        params: mergeParams(
          additionalParams.params,
          buildExportParams(exportState),
        ),
      });
      return;
    }
    if (type === "current_params") {
      onSubmit({
        job_type: "current_params",
        artifact_id: backtestState.artifact_id,
        ...priorityPayload,
        params: mergeParams(
          additionalParams.params,
          buildBacktestParams(backtestState, backtestArtifact),
        ),
      });
      return;
    }
    const sweepParams = mergeParams(
      additionalParams.params,
      buildBacktestParams(sweepState, sweepArtifact),
    );
    sweepParams.sweep_scope = sweepState.sweep_scope;
    sweepParams.sweeps = sweepRows.map(
      (row) => `${row.key.trim()}=${row.value.trim()}`,
    );
    onSubmit({
      job_type: "sweep",
      artifact_id: sweepState.artifact_id,
      ...priorityPayload,
      params: sweepParams,
    });
  };
  const useArtifactFor = (
    nextType: "current_params" | "sweep",
    artifact: ResearchArtifact,
  ) => {
    if (nextType === "current_params") {
      setBacktestState(
        artifactIntervalState({
          ...backtestState,
          artifact_id: artifact.id,
          data_db_path: "",
        }),
      );
    } else {
      setSweepState(
        artifactIntervalState({
          ...sweepState,
          artifact_id: artifact.id,
          data_db_path: "",
        }),
      );
    }
    setType(nextType);
  };
  const handleBacktestFieldsChange = useCallback(
    (next: BacktestState) =>
      type === "current_params"
        ? setBacktestState(next)
        : setSweepState((prev) => ({ ...prev, ...next })),
    [type],
  );

  return (
    <form
      className="space-y-4"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <FormField label="Job type">
        {() =>
          typeLocked ? (
            <div className="border border-border bg-surface px-2 py-1.5 text-sm">
              {jobTypeName(type)}
            </div>
          ) : (
          <Segment
            value={type}
            onChange={(value) => setType(value as JobType)}
            items={[
              { value: "export", label: "Export" },
              { value: "current_params", label: "Backtest" },
              { value: "sweep", label: "Sweep" },
            ]}
            ariaLabel="Job type"
          />
          )
        }
      </FormField>

      {error && (
        <Banner tone="danger" title={errorTitle}>
          {error}
        </Banner>
      )}

      {type === "export" && (
        <ExportFields
          state={exportState}
          artifacts={availableArtifacts}
          onChange={setExportState}
          onUseArtifact={useArtifactFor}
        />
      )}

      {(type === "current_params" || type === "sweep") && (
        <BacktestFields
          kind={type === "current_params" ? "current_params" : "sweep"}
          state={type === "current_params" ? backtestState : sweepState}
          artifact={type === "current_params" ? backtestArtifact : sweepArtifact}
          interval={type === "current_params" ? backtestInterval : sweepInterval}
          artifacts={availableArtifacts}
          onChange={handleBacktestFieldsChange}
        />
      )}

      {type === "sweep" && (
        <SweepFields state={sweepState} onChange={setSweepState} />
      )}

      {showPriority && (
        <FormField
          label="Queue priority"
          hint="Higher numbers run first when several jobs wait. Leave 0 unless you need to jump the queue."
        >
          {({ id }) => (
            <Input
              id={id}
              value={priority}
              inputMode="numeric"
              onChange={(event) => setPriority(event.currentTarget.value)}
            />
          )}
        </FormField>
      )}

      {showAdditionalParams && (
        <FormField
          label="Additional params JSON"
          hint="Unknown source params are preserved here. Known form fields override duplicate keys."
        >
          {({ id }) => (
            <div className="space-y-2">
              <Textarea
                id={id}
                value={additionalParamsJson}
                onChange={(event) =>
                  setAdditionalParamsJson(event.currentTarget.value)
                }
                minRows={5}
              />
              {additionalParams.error && (
                <div className="text-[12px] text-accent-red">
                  {additionalParams.error}
                </div>
              )}
            </div>
          )}
        </FormField>
      )}

      {submitDisabled && submitDisabledReason && (
        <div className="text-[12px] text-muted">{submitDisabledReason}</div>
      )}

      <div className="flex justify-end gap-2">
        {onCancel && (
          <Button onClick={onCancel} disabled={pending}>
            Cancel
          </Button>
        )}
        <Button
          type="submit"
          tone="accent"
          disabled={!isValid || pending || submitDisabled}
          state={pending ? "pending" : "idle"}
        >
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}

interface ExportFieldsProps {
  state: ExportState;
  artifacts: ResearchArtifact[];
  onChange: (next: ExportState) => void;
  onUseArtifact: (
    nextType: "current_params" | "sweep",
    artifact: ResearchArtifact,
  ) => void;
}

function ExportFields({
  state,
  artifacts,
  onChange,
  onUseArtifact,
}: ExportFieldsProps) {
  const exportBlocked = state.run_mode === "live_trading";
  const hasSource = state.source_db_path.trim().length > 0;
  const shownArtifacts = artifacts.slice(0, 4);
  return (
    <div className="space-y-3">
      {shownArtifacts.length > 0 ? (
        <div className="border border-border bg-bg px-3 py-3">
          <div className="font-semibold">Available run artifacts</div>
          <p className="mt-1 text-[12px] text-muted">
            These run exports are already registered. Choose one and start the
            research job directly.
          </p>
          <div className="mt-3 space-y-2">
            {shownArtifacts.map((artifact) => (
              <div
                key={artifact.id}
                className="grid gap-2 border border-border bg-surface px-3 py-2 lg:grid-cols-[minmax(0,1fr)_auto]"
              >
                <div className="min-w-0">
                  <div className="break-all font-mono text-[12px] font-semibold">
                    {artifact.id}
                  </div>
                  <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted">
                    <span>
                      {artifact.interval_start_ms != null
                        ? formatDateTime(artifact.interval_start_ms)
                        : "Unknown start"}
                    </span>
                    <span>
                      {artifact.interval_end_ms != null
                        ? formatDateTime(artifact.interval_end_ms)
                        : "Unknown end"}
                    </span>
                    <span>{formatBytes(artifact.bytes)}</span>
                    {artifact.run_mode && <span>{artifact.run_mode}</span>}
                  </div>
                </div>
                <div className="flex flex-wrap gap-2 lg:justify-end">
                  <Button
                    size="sm"
                    tone="accent"
                    onClick={() => onUseArtifact("current_params", artifact)}
                    aria-label={`Backtest ${artifact.id}`}
                  >
                    Backtest
                  </Button>
                  <Button
                    size="sm"
                    tone="neutral"
                    onClick={() => onUseArtifact("sweep", artifact)}
                    aria-label={`Sweep ${artifact.id}`}
                  >
                    Sweep
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <Banner tone="warning" title="No exported run artifact">
          Create an export from a known stopped-run database, or register an
          artifact from the Artifacts page before backtesting.
        </Banner>
      )}

      <details
        open={artifacts.length === 0 || hasSource}
        className="border border-border bg-bg px-3 py-3 text-[12px]"
      >
        <summary className="cursor-pointer font-semibold">
          Create an export from a known DB
        </summary>
        <div className="mt-3 space-y-3">
          <FormField
            label="Source DB path"
            hint="Absolute path to the stopped run database on the worker host."
            required
          >
            {({ id }) => (
              <Input
                id={id}
                value={state.source_db_path}
                onChange={(event) =>
                  onChange({
                    ...state,
                    source_db_path: event.currentTarget.value,
                  })
                }
                placeholder="/home/.../paint.db"
              />
            )}
          </FormField>
          <div className="border border-border bg-surface px-3 py-2">
            <div className="font-semibold">Export mode</div>
            <label className="mt-2 flex items-start gap-2 text-[13px]">
              <input
                type="checkbox"
                checked={state.dry_run}
                onChange={(event) =>
                  onChange({
                    ...state,
                    dry_run: event.currentTarget.checked,
                    confirm_export: false,
                  })
                }
                className="mt-0.5"
              />
              <span>
                <span className="font-semibold">Dry run only</span>
                <span className="block text-muted">
                  Checks the export plan first. Turn this off only after the
                  dry-run job shows the expected source and output.
                </span>
              </span>
            </label>
            {!state.dry_run && (
              <div className="mt-3 space-y-2 border border-accent-red bg-bg p-3">
                <div className="text-[12px] text-accent-red">
                  Real export writes a new artifact into the research work
                  root. The source database is copied with SQLite backup
                  semantics.
                </div>
                <label className="flex items-start gap-2 text-[13px]">
                  <input
                    type="checkbox"
                    checked={state.confirm_export}
                    onChange={(event) =>
                      onChange({
                        ...state,
                        confirm_export: event.currentTarget.checked,
                      })
                    }
                    className="mt-0.5"
                  />
                  <span>I understand and want to perform a real export.</span>
                </label>
              </div>
            )}
          </div>

          <details className="border border-border bg-surface px-3 py-2">
            <summary className="cursor-pointer font-semibold">
              Advanced export metadata
            </summary>
            <div className="mt-3 space-y-3">
              {exportBlocked && (
                <Banner tone="danger" title="Export blocked for live_trading">
                  Live-trading runs cannot be exported through the research
                  control plane. Use the bot closeout workflow instead.
                </Banner>
              )}
              <div className="grid gap-3 sm:grid-cols-2">
                <FormField label="Run mode" hint="Defaults to live_readonly">
                  {() => (
                    <Segment
                      value={state.run_mode}
                      onChange={(value) =>
                        onChange({
                          ...state,
                          run_mode: value as ExportRunMode,
                        })
                      }
                      items={EXPORT_RUN_MODES.map((mode) => ({
                        value: mode,
                        label: mode,
                      }))}
                      ariaLabel="Run mode"
                    />
                  )}
                </FormField>
                <FormField label="Source state" hint="Defaults to stopped">
                  {() => (
                    <Segment
                      value={state.source_state}
                      onChange={(value) =>
                        onChange({
                          ...state,
                          source_state: value as ExportSourceState,
                        })
                      }
                      items={EXPORT_SOURCE_STATES.map((value) => ({
                        value,
                        label: value,
                      }))}
                      ariaLabel="Source state"
                    />
                  )}
                </FormField>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <FormField label="Interval start" hint="Optional metadata">
                  {({ id }) => (
                    <Input
                      id={id}
                      type="datetime-local"
                      value={state.interval_start_iso}
                      onChange={(event) =>
                        onChange({
                          ...state,
                          interval_start_iso: event.currentTarget.value,
                        })
                      }
                      onBlur={(event) =>
                        onChange({
                          ...state,
                          interval_start_iso: event.currentTarget.value,
                        })
                      }
                    />
                  )}
                </FormField>
                <FormField label="Interval end" hint="Optional metadata">
                  {({ id }) => (
                    <Input
                      id={id}
                      type="datetime-local"
                      value={state.interval_end_iso}
                      onChange={(event) =>
                        onChange({
                          ...state,
                          interval_end_iso: event.currentTarget.value,
                        })
                      }
                      onBlur={(event) =>
                        onChange({
                          ...state,
                          interval_end_iso: event.currentTarget.value,
                        })
                      }
                    />
                  )}
                </FormField>
              </div>
              <FormField
                label="Log paths"
                hint="Optional. Leave blank unless logs need to be bundled with this artifact."
              >
                {({ id }) => (
                  <Textarea
                    id={id}
                    value={state.log_paths}
                    onChange={(event) =>
                      onChange({
                        ...state,
                        log_paths: event.currentTarget.value,
                      })
                    }
                    minRows={3}
                  />
                )}
              </FormField>
            </div>
          </details>
        </div>
      </details>
    </div>
  );
}

function ArtifactSummary({ artifact }: { artifact: ResearchArtifact }) {
  const duration =
    artifact.interval_start_ms != null && artifact.interval_end_ms != null
      ? artifact.interval_end_ms - artifact.interval_start_ms
      : null;
  const quality = [
    artifact.replay_quality_class,
    artifact.backtest_ready_class,
    artifact.live_fidelity_class,
  ].filter((value): value is string => Boolean(value));
  return (
    <div className="border border-border bg-bg px-3 py-2 text-[12px]">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="font-semibold">Artifact context</div>
        <div className="flex flex-wrap gap-1">
          {artifact.run_mode && (
            <StatusChip label={artifact.run_mode} tone="muted" compact />
          )}
          {quality.map((value) => (
            <StatusChip key={value} label={value} tone="success" compact />
          ))}
        </div>
      </div>
      <dl className="mt-2 grid gap-2 sm:grid-cols-3">
        <div>
          <dt className="text-muted">Start</dt>
          <dd>
            {artifact.interval_start_ms != null
              ? formatDateTime(artifact.interval_start_ms)
              : "Unknown"}
          </dd>
        </div>
        <div>
          <dt className="text-muted">End</dt>
          <dd>
            {artifact.interval_end_ms != null
              ? formatDateTime(artifact.interval_end_ms)
              : "Unknown"}
          </dd>
        </div>
        <div>
          <dt className="text-muted">Duration</dt>
          <dd>{duration != null ? formatDurationShort(duration) : "Unknown"}</dd>
        </div>
        <div>
          <dt className="text-muted">Size</dt>
          <dd>{formatBytes(artifact.bytes)}</dd>
        </div>
        <div>
          <dt className="text-muted">Source</dt>
          <dd>{artifact.source_machine_id ?? "Unknown"}</dd>
        </div>
        <div>
          <dt className="text-muted">Input DB</dt>
          <dd className="break-all font-mono text-[11px]">
            {artifact.source_db_path || "Manifest runtime DB"}
          </dd>
        </div>
      </dl>
    </div>
  );
}

interface BacktestFieldsProps {
  kind: "current_params" | "sweep";
  state: BacktestState;
  artifact: ResearchArtifact | undefined;
  interval: EffectiveInterval;
  artifacts: ResearchArtifact[];
  onChange: (next: BacktestState) => void;
}

function BacktestFields({
  kind,
  state,
  artifact,
  interval,
  artifacts,
  onChange,
}: BacktestFieldsProps) {
  const isSweep = kind === "sweep";
  const startInputRef = useRef<HTMLInputElement | null>(null);
  const endInputRef = useRef<HTMLInputElement | null>(null);
  const resetIntervalConfirmation = (next: BacktestState): BacktestState => ({
    ...next,
    confirm_interval: false,
  });

  return (
    <div className="space-y-3">
      <Banner
        tone="info"
        title={isSweep ? "Sweep selected parameters" : "Backtest one parameter set"}
      >
        {isSweep
          ? "A sweep replays one artifact many times while changing selected strategy settings."
          : "A backtest replays one artifact once with the selected settings and starting balance."}
      </Banner>
      {artifacts.length === 0 && (
        <Banner tone="warning" title="No available artifacts">
          Import, register, or export a run artifact before creating a
          backtest or sweep job.
        </Banner>
      )}
      <FormField label="Artifact to replay" required>
        {({ id }) => (
          <select
            id={id}
            value={state.artifact_id}
            onChange={(event) => {
              const artifactId = event.currentTarget.value;
              const nextArtifact = artifacts.find((item) => item.id === artifactId);
              onChange(
                resetIntervalConfirmation({
                  ...state,
                  artifact_id: artifactId,
                  start_iso:
                    state.interval_mode === "custom"
                      ? datetimeLocalFromMs(nextArtifact?.interval_start_ms)
                      : state.start_iso,
                  end_iso:
                    state.interval_mode === "custom"
                      ? datetimeLocalFromMs(nextArtifact?.interval_end_ms)
                      : state.end_iso,
                }),
              );
            }}
            className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
          >
            <option value="">Select an artifact</option>
            {artifacts.map((artifact) => (
              <option key={artifact.id} value={artifact.id}>
                {artifact.id}
              </option>
            ))}
          </select>
        )}
      </FormField>
      {artifact && <ArtifactSummary artifact={artifact} />}
      <FormField label="Replay interval">
        {() => (
          <div className="space-y-2">
            <Segment
              value={state.interval_mode}
              onChange={(value) =>
                onChange(
                  value === "artifact"
                    ? artifactIntervalState(state)
                    : customIntervalState(state, artifact),
                )
              }
              items={[
                { value: "artifact", label: "Full artifact" },
                { value: "custom", label: "Custom range" },
              ]}
              ariaLabel="Replay interval"
            />
            <div className="text-[12px] text-muted">
              {state.interval_mode === "artifact"
                ? "Uses the full artifact interval shown above."
                : "Limits the replay to the selected range inside this artifact."}
            </div>
          </div>
        )}
      </FormField>
      {state.interval_mode === "custom" && (
        <div className="grid gap-3 sm:grid-cols-2">
          <FormField label="Start" required>
            {({ id }) => (
              <Input
                ref={startInputRef}
                id={id}
                type="datetime-local"
                value={state.start_iso}
                min={datetimeLocalFromMs(artifact?.interval_start_ms)}
                max={datetimeLocalFromMs(artifact?.interval_end_ms)}
                onChange={(event) =>
                  onChange(
                    resetIntervalConfirmation({
                      ...state,
                      start_iso: event.currentTarget.value,
                    }),
                  )
                }
                onBlur={(event) =>
                  onChange(
                    resetIntervalConfirmation({
                      ...state,
                      start_iso: event.currentTarget.value,
                    }),
                  )
                }
              />
            )}
          </FormField>
          <FormField label="End" required>
            {({ id }) => (
              <Input
                ref={endInputRef}
                id={id}
                type="datetime-local"
                value={state.end_iso}
                min={datetimeLocalFromMs(artifact?.interval_start_ms)}
                max={datetimeLocalFromMs(artifact?.interval_end_ms)}
                onChange={(event) =>
                  onChange(
                    resetIntervalConfirmation({
                      ...state,
                      end_iso: event.currentTarget.value,
                    }),
                  )
                }
                onBlur={(event) =>
                  onChange(
                    resetIntervalConfirmation({
                      ...state,
                      end_iso: event.currentTarget.value,
                    }),
                  )
                }
              />
            )}
          </FormField>
        </div>
      )}
      <div className="border border-border bg-bg px-3 py-2 text-[12px]">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="font-semibold">Job interval</div>
          <Button
            size="sm"
            onClick={() => onChange(artifactIntervalState(state))}
            disabled={state.interval_mode === "artifact"}
          >
            Use full artifact
          </Button>
        </div>
        {interval.valid &&
        interval.start.ms != null &&
        interval.end.ms != null &&
        interval.durationMs != null ? (
          <dl className="mt-2 grid gap-2 sm:grid-cols-2">
            <div>
              <dt className="text-muted">Start</dt>
              <dd>{formatDateTime(interval.start.ms)}</dd>
            </div>
            <div>
              <dt className="text-muted">End</dt>
              <dd>{formatDateTime(interval.end.ms)}</dd>
            </div>
            <div>
              <dt className="text-muted">Duration</dt>
              <dd>{formatDurationShort(interval.durationMs)}</dd>
            </div>
            <div>
              <dt className="text-muted">Mode</dt>
              <dd>
                {state.interval_mode === "artifact"
                  ? "Full artifact"
                  : "Custom range"}
              </dd>
            </div>
            <div>
              <dt className="text-muted">Start source</dt>
              <dd>{intervalSourceLabel(interval.start.source)}</dd>
            </div>
            <div>
              <dt className="text-muted">End source</dt>
              <dd>{intervalSourceLabel(interval.end.source)}</dd>
            </div>
          </dl>
        ) : (
          <p className="mt-1 text-accent-red">
            {interval.reason ?? "Select an artifact or provide explicit bounds."}
          </p>
        )}
        {artifact && interval.requiresConfirmation && (
          <label className="mt-3 flex items-start gap-2">
            <input
              type="checkbox"
              checked={state.confirm_interval}
              onChange={(event) =>
                onChange({
                  ...state,
                  confirm_interval: event.currentTarget.checked,
                })
              }
              className="mt-0.5"
            />
            <span>
              Confirm this long interval before creating the job. Ranges over
              six hours can take a while on the research worker.
            </span>
          </label>
        )}
      </div>
      <FormField
        label="Starting balance"
        hint="Positive number, default 100"
        required
      >
        {({ id }) => (
          <Input
            id={id}
            value={state.balance}
            inputMode="decimal"
            onChange={(event) =>
              onChange({ ...state, balance: event.currentTarget.value })
            }
          />
        )}
      </FormField>
      <FormField label="Strategy settings">
        {() => (
          <div className="space-y-2">
            <Segment
              value={state.param_source}
              onChange={(value) =>
                onChange({
                  ...state,
                  param_source: value as ParameterSource,
                  setOverrides:
                    value === "custom"
                      ? defaultParameterRows(state.setOverrides)
                      : state.setOverrides,
                })
              }
              items={[
                { value: "worker_defaults", label: "Research defaults" },
                { value: "custom", label: "Custom settings" },
              ]}
              ariaLabel="Strategy settings"
            />
            <div className="text-[12px] text-muted">
              {state.param_source === "custom"
                ? "Selected settings override the research defaults for this job."
                : "Uses the current research defaults. Recorded live-run settings are not reconstructed automatically yet."}
            </div>
          </div>
        )}
      </FormField>
      {state.param_source === "custom" && (
        <FormField label="Custom settings">
          {() => (
            <ParameterRowsEditor
              rows={state.setOverrides}
              onChange={(rows) => onChange({ ...state, setOverrides: rows })}
              addLabel="Add parameter"
              ariaLabel="Parameter overrides"
              valuePlaceholder="value"
            />
          )}
        </FormField>
      )}
      <details className="border border-border bg-bg px-3 py-2 text-[12px]">
        <summary className="cursor-pointer font-semibold">
          Advanced artifact input
        </summary>
        <div className="mt-3">
          <FormField
            label="Data DB path override"
            hint="Optional. Leave blank to use the runtime DB from the artifact manifest."
          >
            {({ id }) => (
              <Input
                id={id}
                value={state.data_db_path}
                onChange={(event) =>
                  onChange({
                    ...state,
                    data_db_path: event.currentTarget.value,
                  })
                }
                placeholder="/research/.../paint.db"
              />
            )}
          </FormField>
        </div>
      </details>
    </div>
  );
}

interface SweepFieldsProps {
  state: SweepState;
  onChange: (next: SweepState) => void;
}

function SweepFields({ state, onChange }: SweepFieldsProps) {
  const activeRows = sweepRowsForState(state);
  const combinations = sweepCombinationCount(activeRows);
  return (
    <div className="space-y-3">
      <FormField label="Sweep breadth">
        {() => (
          <div className="space-y-2">
            <Segment
              value={state.sweep_scope}
              onChange={(value) =>
                onChange({
                  ...state,
                  sweep_scope: value as SweepScope,
                  sweeps:
                    value === "full"
                      ? DEFAULT_SWEEP_ROWS
                      : defaultParameterRows(
                          state.sweeps.length > 0
                            ? state.sweeps
                            : DEFAULT_SWEEP_ROWS.slice(0, 1),
                        ),
                })
              }
              items={[
                { value: "full", label: "Full sweep preset" },
                { value: "focused", label: "Focused ranges" },
              ]}
              ariaLabel="Sweep breadth"
            />
            <div className="text-[12px] text-muted">
              {state.sweep_scope === "full"
                ? "Tests the main latency-arbitrage thresholds and simulated order latency across the default grid."
                : "Tests only the ranges selected below."}
            </div>
          </div>
        )}
      </FormField>
      {state.sweep_scope === "focused" && (
        <FormField
          label="Focused parameter ranges"
          hint="Each value is a comma-separated range, for example 0.30,0.35,0.40."
          required
        >
          {() => (
            <ParameterRowsEditor
              rows={state.sweeps}
              onChange={(rows) => onChange({ ...state, sweeps: rows })}
              addLabel="Add parameter range"
              ariaLabel="Sweep dimensions"
              valuePlaceholder="0.30,0.35,0.40"
            />
          )}
        </FormField>
      )}
      {state.sweep_scope === "full" && (
        <div className="border border-border bg-bg px-3 py-2 text-[12px]">
          <div className="font-semibold">Full sweep preset</div>
          <ul className="mt-2 space-y-1">
            {DEFAULT_SWEEP_ROWS.map((row) => (
              <li key={row.key}>
                <span className="text-muted">
                  {parameterOption(row.key)?.label ?? row.key}:
                </span>{" "}
                <span className="font-mono">{row.value}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="border border-border bg-bg px-3 py-2 text-[12px]">
        <div className="font-semibold">Sweep size</div>
        <div className="mt-1 text-muted">
          {activeRows.length === 0
            ? "Add at least one parameter range."
            : `${combinations} parameter combination${
                combinations === 1 ? "" : "s"
              } will run.`}
        </div>
      </div>
    </div>
  );
}

interface ParameterRowsEditorProps {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  addLabel: string;
  ariaLabel: string;
  valuePlaceholder: string;
}

function ParameterRowsEditor({
  rows,
  onChange,
  addLabel,
  ariaLabel,
  valuePlaceholder,
}: ParameterRowsEditorProps) {
  const update = (index: number, patch: Partial<KeyValueRow>) => {
    onChange(
      rows.map((row, idx) => (idx === index ? { ...row, ...patch } : row)),
    );
  };
  const remove = (index: number) => {
    onChange(rows.filter((_, idx) => idx !== index));
  };
  const add = () => {
    onChange([...rows, { key: "", value: "" }]);
  };

  return (
    <div className="space-y-2" role="group" aria-label={ariaLabel}>
      {rows.length === 0 && (
        <div className="text-[11px] text-muted">No parameters yet.</div>
      )}
      {rows.map((row, index) => {
        const option = parameterOption(row.key);
        const currentKeyKnown = row.key === "" || option != null;
        return (
          <div
            key={index}
            className="grid gap-2 border border-border bg-surface px-3 py-2 lg:grid-cols-[minmax(18rem,1.3fr)_minmax(14rem,1fr)_auto]"
          >
            <div className="min-w-0 space-y-1">
              <div className="text-[11px] font-semibold text-muted">
                Parameter
              </div>
              <select
                value={row.key}
                aria-label={`${ariaLabel} parameter ${index + 1}`}
                onChange={(event) =>
                  update(index, { key: event.currentTarget.value })
                }
                className="min-h-[40px] w-full border border-border bg-bg px-2 py-2 text-sm"
              >
                <option value="">Select parameter</option>
                {!currentKeyKnown && (
                  <option value={row.key}>{row.key}</option>
                )}
                {PARAMETER_OPTIONS.map((option) => (
                  <option key={option.key} value={option.key}>
                    {option.label}
                  </option>
                ))}
              </select>
              {(option?.hint || !currentKeyKnown) && (
                <div className="text-[11px] text-muted">
                  {option?.hint ?? "Custom parameter preserved from source job."}
                </div>
              )}
            </div>
            <div className="min-w-0 space-y-1">
              <div className="text-[11px] font-semibold text-muted">
                Value or range
              </div>
              <Input
                value={row.value}
                placeholder={option?.placeholder ?? valuePlaceholder}
                aria-label={`${ariaLabel} value ${index + 1}`}
                onChange={(event) =>
                  update(index, { value: event.currentTarget.value })
                }
              />
            </div>
            <Button
              size="sm"
              tone="neutral"
              onClick={() => remove(index)}
              aria-label={`Remove parameter ${index + 1}`}
              className="self-end"
            >
              Remove
            </Button>
          </div>
        );
      })}
      <Button size="sm" tone="neutral" onClick={add}>
        {addLabel}
      </Button>
    </div>
  );
}
