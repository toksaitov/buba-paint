import { useMemo } from "react";
import {
  Banner,
  Button,
} from "../ui/dashboard-primitives";
import { Dialog } from "../ui/dialog";
import { JobCreateForm, type JobCreateFormInitialValues } from "./job-create-form";
import type { KeyValueRow } from "./key-value-editor";
import type {
  CloneJobRequest,
  CreateJobRequest,
  ResearchArtifact,
  ResearchJob,
} from "../../lib/research-types";

interface JobCloneDialogProps {
  open: boolean;
  job: ResearchJob;
  artifacts: ResearchArtifact[];
  loadingArtifacts: boolean;
  artifactError: string | null;
  role: "admin" | "observer" | undefined;
  pending: boolean;
  error: string | null;
  onSubmit: (req: CloneJobRequest) => void;
  onClose: () => void;
}

const BACKTEST_KNOWN_KEYS = new Set([
  "artifact_id",
  "data_db_path",
  "start",
  "start_ms",
  "end",
  "end_ms",
  "balance",
  "set",
]);

const SWEEP_KNOWN_KEYS = new Set([...BACKTEST_KNOWN_KEYS, "sweeps"]);

const EXPORT_KNOWN_KEYS = new Set([
  "source_db_path",
  "run_mode",
  "source_state",
  "interval_start_ms",
  "interval_end_ms",
  "log_paths",
  "dry_run",
  "confirm_export",
]);

function parseRecord(value: string | null): Record<string, unknown> {
  if (!value) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    if (
      parsed == null ||
      typeof parsed !== "object" ||
      Array.isArray(parsed)
    ) {
      return {};
    }
    return parsed as Record<string, unknown>;
  } catch {
    return {};
  }
}

function datetimeLocalFromMs(value: unknown): string {
  const ms = typeof value === "number" ? value : Date.parse(String(value));
  if (!Number.isFinite(ms)) return "";
  const date = new Date(ms);
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

function stringValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "";
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function keyValueRows(value: unknown): KeyValueRow[] {
  if (Array.isArray(value)) {
    return value.flatMap((entry) => {
      if (typeof entry !== "string") return [];
      const index = entry.indexOf("=");
      if (index <= 0) return [];
      return [{ key: entry.slice(0, index), value: entry.slice(index + 1) }];
    });
  }
  if (value != null && typeof value === "object") {
    return Object.entries(value).map(([key, entry]) => ({
      key,
      value: String(entry),
    }));
  }
  return [];
}

function additionalJson(
  params: Record<string, unknown>,
  known: Set<string>,
): string {
  const additional = Object.fromEntries(
    Object.entries(params).filter(([key]) => !known.has(key)),
  );
  return Object.keys(additional).length > 0
    ? JSON.stringify(additional, null, 2)
    : "";
}

function exportRunMode(value: unknown) {
  if (
    value === "paper" ||
    value === "live_readonly" ||
    value === "live_trading"
  ) {
    return value;
  }
  return undefined;
}

function exportSourceState(value: unknown) {
  if (value === "stopped" || value === "running_readonly") {
    return value;
  }
  return undefined;
}

function initialValuesFromJob(job: ResearchJob): JobCreateFormInitialValues {
  const params = parseRecord(job.params_json);
  const priority = job.priority;
  if (job.job_type === "export") {
    return {
      type: job.job_type,
      priority,
      additionalParamsJson: additionalJson(params, EXPORT_KNOWN_KEYS),
      export: {
        source_db_path: stringValue(params.source_db_path),
        run_mode: exportRunMode(params.run_mode),
        source_state: exportSourceState(params.source_state),
        interval_start_iso: datetimeLocalFromMs(params.interval_start_ms),
        interval_end_iso: datetimeLocalFromMs(params.interval_end_ms),
        log_paths: Array.isArray(params.log_paths)
          ? params.log_paths.map(String).join("\n")
          : stringValue(params.log_paths),
        dry_run: booleanValue(params.dry_run, true),
        confirm_export: booleanValue(params.confirm_export, false),
      },
    };
  }

  const base = {
    artifact_id: job.artifact_id ?? stringValue(params.artifact_id),
    data_db_path: stringValue(params.data_db_path),
    start_iso: datetimeLocalFromMs(params.start_ms ?? params.start),
    end_iso: datetimeLocalFromMs(params.end_ms ?? params.end),
    balance: stringValue(params.balance) || "200",
    setOverrides: keyValueRows(params.set),
  };

  if (job.job_type === "sweep") {
    return {
      type: job.job_type,
      priority,
      additionalParamsJson: additionalJson(params, SWEEP_KNOWN_KEYS),
      sweep: {
        ...base,
        sweeps: keyValueRows(params.sweeps),
      },
    };
  }

  return {
    type: "current_params",
    priority,
    additionalParamsJson: additionalJson(params, BACKTEST_KNOWN_KEYS),
    backtest: base,
  };
}

export function JobCloneDialog({
  open,
  job,
  artifacts,
  loadingArtifacts,
  artifactError,
  role,
  pending,
  error,
  onSubmit,
  onClose,
}: JobCloneDialogProps) {
  const initialValues = useMemo(() => initialValuesFromJob(job), [job]);
  const canSubmit = role === "admin";
  const submitClone = (req: CreateJobRequest) => {
    onSubmit({
      artifact_id: req.artifact_id,
      priority: req.priority,
      params: req.params ?? null,
    });
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Clone job"
      description="Create a new queued job from this job, with explicit edits before submission."
      width="lg"
    >
      <div className="space-y-4 p-4">
        <div className="space-y-1 text-[12px] text-muted">
          <div>
            Source job:{" "}
            <span className="font-mono text-text">{job.id}</span>
          </div>
          <div>
            Clone keeps the original job type. Known fields are editable below;
            unknown source params remain in Additional params JSON.
          </div>
        </div>
        {loadingArtifacts ? (
          <div className="text-[12px] text-muted">Loading artifacts...</div>
        ) : artifactError ? (
          <Banner tone="danger" title="Could not load artifacts">
            {artifactError}
          </Banner>
        ) : (
          <JobCreateForm
            artifacts={artifacts}
            initialType={job.job_type}
            initialValues={initialValues}
            typeLocked
            showPriority
            showAdditionalParams
            submitLabel="Create clone"
            submitDisabled={!canSubmit}
            submitDisabledReason={
              canSubmit ? undefined : "Admin role required to create the clone."
            }
            errorTitle="Clone failed"
            pending={pending}
            error={error}
            onSubmit={submitClone}
            onCancel={onClose}
          />
        )}
        {artifactError && (
          <div className="flex justify-end">
            <Button onClick={onClose}>Close</Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
