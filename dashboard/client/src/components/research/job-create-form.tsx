import { useCallback, useMemo, useState } from "react";
import {
  Banner,
  Button,
  FormField,
  Input,
  Segment,
  Textarea,
} from "../ui/dashboard-primitives";
import type {
  CreateJobRequest,
  JobType,
  ResearchArtifact,
} from "../../lib/research-types";
import {
  artifactIntervalState,
  artifactRecency,
  buildBacktestParams,
  buildExportParams,
  defaultParameterRows,
  effectiveInterval,
  initialIntervalMode,
  jobTypeName,
  mergeParams,
  parseAdditionalParams,
  sweepRowsForState,
  DEFAULT_SWEEP_ROWS,
  DEFAULT_STARTING_BALANCE,
  type BacktestState,
  type ExportState,
  type JobCreateFormInitialValues,
  type SweepState,
} from "./job-create-params";
import { ExportFields } from "./job-create/export-fields";
import { BacktestFields } from "./job-create/backtest-fields";
import { SweepFields } from "./job-create/sweep-fields";

export type { JobCreateFormInitialValues } from "./job-create-params";

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
