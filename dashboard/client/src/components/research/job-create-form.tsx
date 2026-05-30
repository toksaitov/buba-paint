import { useEffect, useMemo, useRef, useState } from "react";
import {
  Banner,
  Button,
  FormField,
  Input,
  Segment,
  Textarea,
} from "../ui/dashboard-primitives";
import { KeyValueEditor, type KeyValueRow } from "./key-value-editor";
import type {
  CreateJobRequest,
  JobType,
  ResearchArtifact,
} from "../../lib/research-types";
import { formatDateTime, formatDurationShort } from "../../lib/utils";

const EXPORT_RUN_MODES = ["paper", "live_readonly", "live_trading"] as const;
const EXPORT_SOURCE_STATES = ["stopped", "running_readonly"] as const;
const LARGE_INTERVAL_MS = 6 * 60 * 60 * 1000;

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
  start_iso: string;
  end_iso: string;
  balance: string;
  confirm_interval: boolean;
  setOverrides: KeyValueRow[];
}

interface SweepState extends BacktestState {
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
  const start = resolveIntervalBoundary(
    state.start_iso,
    artifact?.interval_start_ms,
  );
  const end = resolveIntervalBoundary(state.end_iso, artifact?.interval_end_ms);
  let reason: string | null = null;
  let durationMs: number | undefined;
  if (start.source === "invalid" || end.source === "invalid") {
    reason = "Start and end must be valid datetimes.";
  } else if (start.ms == null || end.ms == null) {
    reason = "Start and end are required, either explicitly or from artifact metadata.";
  } else if (end.ms <= start.ms) {
    reason = "End must be after start.";
  } else {
    durationMs = end.ms - start.ms;
  }
  const fallbackDerived =
    start.source === "artifact fallback" || end.source === "artifact fallback";
  return {
    start,
    end,
    durationMs,
    valid: reason == null,
    reason,
    requiresConfirmation:
      fallbackDerived || (durationMs != null && durationMs > LARGE_INTERVAL_MS),
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
  const params: Record<string, unknown> = {};
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
  const sets = state.setOverrides
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
    () => artifacts.filter((a) => a.status === "available"),
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

  const [backtestState, setBacktestState] = useState<BacktestState>({
    artifact_id: initialValues?.backtest?.artifact_id ?? initialArtifactId,
    data_db_path: initialValues?.backtest?.data_db_path ?? "",
    start_iso: initialValues?.backtest?.start_iso ?? "",
    end_iso: initialValues?.backtest?.end_iso ?? "",
    balance: initialValues?.backtest?.balance ?? "200",
    confirm_interval: initialValues?.backtest?.confirm_interval ?? false,
    setOverrides: initialValues?.backtest?.setOverrides ?? [],
  });

  const [sweepState, setSweepState] = useState<SweepState>({
    artifact_id: initialValues?.sweep?.artifact_id ?? initialArtifactId,
    data_db_path: initialValues?.sweep?.data_db_path ?? "",
    start_iso: initialValues?.sweep?.start_iso ?? "",
    end_iso: initialValues?.sweep?.end_iso ?? "",
    balance: initialValues?.sweep?.balance ?? "200",
    confirm_interval: initialValues?.sweep?.confirm_interval ?? false,
    setOverrides: initialValues?.sweep?.setOverrides ?? [],
    sweeps: initialValues?.sweep?.sweeps ?? [],
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
  const sweepRows = sweepState.sweeps.filter(
    (row) => row.key.trim() && row.value.trim(),
  );
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

      {showPriority && (
        <FormField label="Priority" hint="Integer queue priority">
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

      {error && (
        <Banner tone="danger" title={errorTitle}>
          {error}
        </Banner>
      )}

      {type === "export" && (
        <div className="space-y-3">
          {exportBlocked && (
            <Banner tone="danger" title="Export blocked for live_trading">
              Live-trading runs cannot be exported through the research
              control plane. Use the bot closeout workflow instead.
            </Banner>
          )}
          <FormField label="Source database path" required>
            {({ id }) => (
              <Input
                id={id}
                value={exportState.source_db_path}
                onChange={(event) =>
                  setExportState({
                    ...exportState,
                    source_db_path: event.currentTarget.value,
                  })
                }
                placeholder="/absolute/path/to/paint.db"
              />
            )}
          </FormField>
          <div className="grid gap-3 sm:grid-cols-2">
            <FormField label="Run mode">
              {() => (
                <Segment
                  value={exportState.run_mode}
                  onChange={(value) =>
                    setExportState({
                      ...exportState,
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
            <FormField label="Source state">
              {() => (
                <Segment
                  value={exportState.source_state}
                  onChange={(value) =>
                    setExportState({
                      ...exportState,
                      source_state: value as ExportSourceState,
                    })
                  }
                  items={EXPORT_SOURCE_STATES.map((state) => ({
                    value: state,
                    label: state,
                  }))}
                  ariaLabel="Source state"
                />
              )}
            </FormField>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <FormField
              label="Interval start"
              hint="Optional ISO datetime"
            >
              {({ id }) => (
                <Input
                  id={id}
                  type="datetime-local"
                  value={exportState.interval_start_iso}
                onChange={(event) =>
                  setExportState({
                    ...exportState,
                    interval_start_iso: event.currentTarget.value,
                  })
                }
                onBlur={(event) =>
                  setExportState({
                    ...exportState,
                    interval_start_iso: event.currentTarget.value,
                  })
                }
              />
            )}
          </FormField>
            <FormField label="Interval end" hint="Optional ISO datetime">
              {({ id }) => (
                <Input
                  id={id}
                  type="datetime-local"
                  value={exportState.interval_end_iso}
                onChange={(event) =>
                  setExportState({
                    ...exportState,
                    interval_end_iso: event.currentTarget.value,
                  })
                }
                onBlur={(event) =>
                  setExportState({
                    ...exportState,
                    interval_end_iso: event.currentTarget.value,
                  })
                }
              />
            )}
          </FormField>
          </div>
          <FormField label="Log paths" hint="One path per line, optional">
            {({ id }) => (
              <Textarea
                id={id}
                value={exportState.log_paths}
                onChange={(event) =>
                  setExportState({
                    ...exportState,
                    log_paths: event.currentTarget.value,
                  })
                }
                minRows={3}
              />
            )}
          </FormField>
          <label className="flex items-start gap-2 text-[13px]">
            <input
              type="checkbox"
              checked={exportState.dry_run}
              onChange={(event) =>
                setExportState({
                  ...exportState,
                  dry_run: event.currentTarget.checked,
                  confirm_export: false,
                })
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-semibold">Dry run only</span>
              <span className="block text-muted">
                Plans the export without writing artifact files. Recommended
                default.
              </span>
            </span>
          </label>
          {!exportState.dry_run && (
            <div className="space-y-2 border border-accent-red bg-bg p-3">
              <div className="text-[12px] text-accent-red">
                Disabling dry-run will write artifact files to the configured
                research work root. This is not reversible from the dashboard.
              </div>
              <label className="flex items-start gap-2 text-[13px]">
                <input
                  type="checkbox"
                  checked={exportState.confirm_export}
                  onChange={(event) =>
                    setExportState({
                      ...exportState,
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
      )}

      {(type === "current_params" || type === "sweep") && (
        <BacktestFields
          state={type === "current_params" ? backtestState : sweepState}
          artifact={type === "current_params" ? backtestArtifact : sweepArtifact}
          interval={type === "current_params" ? backtestInterval : sweepInterval}
          artifacts={availableArtifacts}
          onChange={(next) =>
            type === "current_params"
              ? setBacktestState(next)
              : setSweepState({ ...sweepState, ...next })
          }
        />
      )}

      {type === "sweep" && (
        <FormField
          label="Sweep dimensions"
          hint="At least one row. Value is a comma-separated list, e.g. 1.0,2.5,4.0."
          required
        >
          {() => (
            <KeyValueEditor
              rows={sweepState.sweeps}
              onChange={(rows) =>
                setSweepState({ ...sweepState, sweeps: rows })
              }
              keyPlaceholder="parameter"
              valuePlaceholder="1.0,2.5,4.0"
              addLabel="Add sweep dimension"
              ariaLabel="Sweep dimensions"
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

interface BacktestFieldsProps {
  state: BacktestState;
  artifact: ResearchArtifact | undefined;
  interval: EffectiveInterval;
  artifacts: ResearchArtifact[];
  onChange: (next: BacktestState) => void;
}

function BacktestFields({
  state,
  artifact,
  interval,
  artifacts,
  onChange,
}: BacktestFieldsProps) {
  const startInputRef = useRef<HTMLInputElement | null>(null);
  const endInputRef = useRef<HTMLInputElement | null>(null);
  const sourceLabel = [interval.start.source, interval.end.source]
    .filter((value, index, values) => values.indexOf(value) === index)
    .join(", ");
  const resetIntervalConfirmation = (next: BacktestState): BacktestState => ({
    ...next,
    confirm_interval: false,
  });
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      const startValue = startInputRef.current?.value ?? state.start_iso;
      const endValue = endInputRef.current?.value ?? state.end_iso;
      if (startValue === state.start_iso && endValue === state.end_iso) {
        return;
      }
      onChange({
        ...state,
        start_iso: startValue,
        end_iso: endValue,
        confirm_interval: false,
      });
    }, 250);
    return () => window.clearInterval(intervalId);
  }, [onChange, state]);

  return (
    <div className="space-y-3">
      <FormField label="Source artifact" required>
        {({ id }) => (
          <select
            id={id}
            value={state.artifact_id}
            onChange={(event) =>
              onChange(
                resetIntervalConfirmation({
                  ...state,
                  artifact_id: event.currentTarget.value,
                }),
              )
            }
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
      <FormField label="Data DB path override" hint="Optional">
        {({ id }) => (
          <Input
            id={id}
            value={state.data_db_path}
            onChange={(event) =>
              onChange({ ...state, data_db_path: event.currentTarget.value })
            }
            placeholder="/research/.../paint.db"
          />
        )}
      </FormField>
      <div className="grid gap-3 sm:grid-cols-2">
        <FormField label="Start" hint="Falls back to artifact interval">
          {({ id }) => (
            <Input
              ref={startInputRef}
              id={id}
              type="datetime-local"
              value={state.start_iso}
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
        <FormField label="End" hint="Falls back to artifact interval">
          {({ id }) => (
            <Input
              ref={endInputRef}
              id={id}
              type="datetime-local"
              value={state.end_iso}
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
      <div className="border border-border bg-bg px-3 py-2 text-[12px]">
        <div className="font-semibold">Effective interval</div>
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
              <dt className="text-muted">Source</dt>
              <dd>{sourceLabel}</dd>
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
              Confirm this interval before creating the job. Confirmation is
              required for artifact-derived or large ranges.
            </span>
          </label>
        )}
      </div>
      <FormField label="Balance" hint="Positive number, default 200" required>
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
      <FormField
        label="--set overrides"
        hint="Optional runtime config overrides"
      >
        {() => (
          <KeyValueEditor
            rows={state.setOverrides}
            onChange={(rows) => onChange({ ...state, setOverrides: rows })}
            keyPlaceholder="KEY"
            valuePlaceholder="value"
            addLabel="Add override"
            ariaLabel="Set overrides"
          />
        )}
      </FormField>
    </div>
  );
}
