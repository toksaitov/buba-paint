import { useMemo, useState } from "react";
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

const EXPORT_RUN_MODES = ["paper", "live_readonly", "live_trading"] as const;
const EXPORT_SOURCE_STATES = ["stopped", "running_readonly"] as const;

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
  setOverrides: KeyValueRow[];
}

interface SweepState extends BacktestState {
  sweeps: KeyValueRow[];
}

interface JobCreateFormProps {
  artifacts: ResearchArtifact[];
  initialType?: JobType;
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
  const start =
    isoToMs(state.start_iso) ?? artifact?.interval_start_ms ?? undefined;
  const end = isoToMs(state.end_iso) ?? artifact?.interval_end_ms ?? undefined;
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

export function JobCreateForm({
  artifacts,
  initialType = "current_params",
  pending,
  error,
  onSubmit,
  onCancel,
}: JobCreateFormProps) {
  const availableArtifacts = useMemo(
    () => artifacts.filter((a) => a.status === "available"),
    [artifacts],
  );

  const [type, setType] = useState<JobType>(initialType);

  const [exportState, setExportState] = useState<ExportState>({
    source_db_path: "",
    run_mode: "live_readonly",
    source_state: "stopped",
    interval_start_iso: "",
    interval_end_iso: "",
    log_paths: "",
    dry_run: true,
    confirm_export: false,
  });

  const initialArtifactId = availableArtifacts[0]?.id ?? "";

  const [backtestState, setBacktestState] = useState<BacktestState>({
    artifact_id: initialArtifactId,
    data_db_path: "",
    start_iso: "",
    end_iso: "",
    balance: "200",
    setOverrides: [],
  });

  const [sweepState, setSweepState] = useState<SweepState>({
    artifact_id: initialArtifactId,
    data_db_path: "",
    start_iso: "",
    end_iso: "",
    balance: "200",
    setOverrides: [],
    sweeps: [],
  });

  const exportBlocked = exportState.run_mode === "live_trading";
  const exportConfirmed = exportState.dry_run || exportState.confirm_export;
  const exportValid =
    !exportBlocked &&
    exportState.source_db_path.trim().length > 0 &&
    exportConfirmed;

  const backtestArtifact = availableArtifacts.find(
    (a) => a.id === backtestState.artifact_id,
  );
  const backtestValid =
    backtestState.artifact_id.trim().length > 0 &&
    Number.isFinite(Number(backtestState.balance)) &&
    Number(backtestState.balance) > 0;

  const sweepArtifact = availableArtifacts.find(
    (a) => a.id === sweepState.artifact_id,
  );
  const sweepRows = sweepState.sweeps.filter(
    (row) => row.key.trim() && row.value.trim(),
  );
  const sweepValid =
    sweepState.artifact_id.trim().length > 0 &&
    sweepRows.length >= 1 &&
    Number.isFinite(Number(sweepState.balance)) &&
    Number(sweepState.balance) > 0;

  const isValid =
    type === "export"
      ? exportValid
      : type === "current_params"
        ? backtestValid
        : sweepValid;

  const submit = () => {
    if (!isValid || pending) return;
    if (type === "export") {
      onSubmit({
        job_type: "export",
        params: buildExportParams(exportState),
      });
      return;
    }
    if (type === "current_params") {
      onSubmit({
        job_type: "current_params",
        artifact_id: backtestState.artifact_id,
        params: buildBacktestParams(backtestState, backtestArtifact),
      });
      return;
    }
    const sweepParams = buildBacktestParams(sweepState, sweepArtifact);
    sweepParams.sweeps = sweepRows.map(
      (row) => `${row.key.trim()}=${row.value.trim()}`,
    );
    onSubmit({
      job_type: "sweep",
      artifact_id: sweepState.artifact_id,
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
        {() => (
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
        )}
      </FormField>

      {error && (
        <Banner tone="danger" title="Job creation failed">
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

      <div className="flex justify-end gap-2">
        {onCancel && (
          <Button onClick={onCancel} disabled={pending}>
            Cancel
          </Button>
        )}
        <Button
          type="submit"
          tone="accent"
          disabled={!isValid || pending}
          state={pending ? "pending" : "idle"}
        >
          Create job
        </Button>
      </div>
    </form>
  );
}

interface BacktestFieldsProps {
  state: BacktestState;
  artifacts: ResearchArtifact[];
  onChange: (next: BacktestState) => void;
}

function BacktestFields({ state, artifacts, onChange }: BacktestFieldsProps) {
  return (
    <div className="space-y-3">
      <FormField label="Source artifact" required>
        {({ id }) => (
          <select
            id={id}
            value={state.artifact_id}
            onChange={(event) =>
              onChange({ ...state, artifact_id: event.currentTarget.value })
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
              id={id}
              type="datetime-local"
              value={state.start_iso}
              onChange={(event) =>
                onChange({ ...state, start_iso: event.currentTarget.value })
              }
            />
          )}
        </FormField>
        <FormField label="End" hint="Falls back to artifact interval">
          {({ id }) => (
            <Input
              id={id}
              type="datetime-local"
              value={state.end_iso}
              onChange={(event) =>
                onChange({ ...state, end_iso: event.currentTarget.value })
              }
            />
          )}
        </FormField>
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
