import type { KeyValueRow } from "./key-value-editor";
import type { JobCreateFormInitialValues } from "./job-create-form";
import type {
  ResearchJob,
  ResearchJobTemplate,
} from "../../lib/research-types";

const BACKTEST_KNOWN_KEYS = new Set([
  "artifact_id",
  "data_db_path",
  "start",
  "start_ms",
  "end",
  "end_ms",
  "balance",
  "set",
  "set_overrides",
]);

const SWEEP_KNOWN_KEYS = new Set([
  ...BACKTEST_KNOWN_KEYS,
  "sweep",
  "sweep_dimensions",
  "sweeps",
]);

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

export function parseRecord(value: string | null): Record<string, unknown> {
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

function firstDefined(...values: unknown[]): unknown {
  return values.find((value) => value !== undefined);
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

export function initialValuesFromJob(job: ResearchJob): JobCreateFormInitialValues {
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

  return initialValuesFromBacktestParams(job.job_type, job.priority, job.artifact_id, params);
}

export function initialValuesFromTemplate(
  template: ResearchJobTemplate,
): JobCreateFormInitialValues {
  const params = parseRecord(template.params_json);
  return initialValuesFromBacktestParams(
    template.job_type,
    template.priority,
    template.artifact_id,
    params,
  );
}

function initialValuesFromBacktestParams(
  type: "current_params" | "sweep",
  priority: number,
  artifactId: string | null,
  params: Record<string, unknown>,
): JobCreateFormInitialValues {
  const base = {
    artifact_id: artifactId ?? stringValue(params.artifact_id),
    data_db_path: stringValue(params.data_db_path),
    start_iso: datetimeLocalFromMs(params.start_ms ?? params.start),
    end_iso: datetimeLocalFromMs(params.end_ms ?? params.end),
    balance: stringValue(params.balance) || "200",
    setOverrides: keyValueRows(
      firstDefined(params.set, params.set_overrides),
    ),
  };

  if (type === "sweep") {
    return {
      type,
      priority,
      additionalParamsJson: additionalJson(params, SWEEP_KNOWN_KEYS),
      sweep: {
        ...base,
        sweeps: keyValueRows(
          firstDefined(params.sweeps, params.sweep, params.sweep_dimensions),
        ),
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
