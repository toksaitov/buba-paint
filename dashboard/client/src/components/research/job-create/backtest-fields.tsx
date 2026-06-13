import {
  Banner,
  Button,
  FormField,
  Input,
  Segment,
  StatusChip,
} from "../../ui/dashboard-primitives";
import type { ResearchArtifact } from "../../../lib/research-types";
import {
  formatBytes,
  formatDateTime,
  formatDurationShort,
} from "../../../lib/utils";
import {
  artifactIntervalState,
  customIntervalState,
  datetimeLocalFromMs,
  defaultParameterRows,
  intervalSourceLabel,
  type BacktestState,
  type EffectiveInterval,
  type ParameterSource,
} from "../job-create-params";
import { ParameterRowsEditor } from "./parameter-rows-editor";

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

export function BacktestFields({
  kind,
  state,
  artifact,
  interval,
  artifacts,
  onChange,
}: BacktestFieldsProps) {
  const isSweep = kind === "sweep";
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
