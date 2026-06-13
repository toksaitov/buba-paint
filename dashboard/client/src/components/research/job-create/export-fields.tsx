import {
  Banner,
  Button,
  FormField,
  Input,
  Segment,
  Textarea,
} from "../../ui/dashboard-primitives";
import type { ResearchArtifact } from "../../../lib/research-types";
import { formatBytes, formatDateTime } from "../../../lib/utils";
import {
  EXPORT_RUN_MODES,
  EXPORT_SOURCE_STATES,
  type ExportRunMode,
  type ExportSourceState,
  type ExportState,
} from "../job-create-params";

interface ExportFieldsProps {
  state: ExportState;
  artifacts: ResearchArtifact[];
  onChange: (next: ExportState) => void;
  onUseArtifact: (
    nextType: "current_params" | "sweep",
    artifact: ResearchArtifact,
  ) => void;
}

export function ExportFields({
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
