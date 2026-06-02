import {
  Banner,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StatusChip,
} from "../ui/dashboard-primitives";
import { JsonViewer } from "./json-viewer";
import { isStepLeaseExpired, stepTone } from "../../lib/research-permissions";
import type {
  ResearchJob,
  ResearchJobEvent,
  ResearchJobStep,
} from "../../lib/research-types";
import { formatDateTime, humanize } from "../../lib/utils";

interface JobRecoveryDiagnosisProps {
  job: ResearchJob;
  steps: ResearchJobStep[];
  events: ResearchJobEvent[];
}

interface CommandSpec {
  program?: string;
  args?: string[];
  cwd?: string;
}

interface CommandOutput {
  status_code?: number | null;
  stdout?: string;
  stderr?: string;
  success?: boolean;
  cancelled?: boolean;
}

interface RecoveryContext {
  step: ResearchJobStep;
  event: ResearchJobEvent | null;
  details: Record<string, unknown> | null;
  command: CommandSpec | null;
  output: CommandOutput | null;
  staleLease: boolean;
  guidance: string[];
}

function parseRecord(value: string | null): Record<string, unknown> | null {
  if (!value) return null;
  try {
    const parsed: unknown = JSON.parse(value);
    if (
      parsed == null ||
      typeof parsed !== "object" ||
      Array.isArray(parsed)
    ) {
      return null;
    }
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

function stringArray(value: unknown): string[] | undefined {
  return Array.isArray(value) && value.every((entry) => typeof entry === "string")
    ? value
    : undefined;
}

function commandFromRecord(record: Record<string, unknown> | null): CommandSpec | null {
  if (!record) return null;
  const nested = parseNestedRecord(record.command);
  const source = nested ?? record;
  const program =
    typeof source.program === "string" ? source.program : undefined;
  const args = stringArray(source.args);
  const cwd = typeof source.cwd === "string" ? source.cwd : undefined;
  return program || args || cwd ? { program, args, cwd } : null;
}

function outputFromRecord(record: Record<string, unknown> | null): CommandOutput | null {
  if (!record) return null;
  const output = parseNestedRecord(record.command_output);
  if (!output) return null;
  const statusCode =
    typeof output.status_code === "number" ? output.status_code : null;
  const stdout = typeof output.stdout === "string" ? output.stdout : "";
  const stderr = typeof output.stderr === "string" ? output.stderr : "";
  const success =
    typeof output.success === "boolean" ? output.success : undefined;
  const cancelled =
    typeof output.cancelled === "boolean" ? output.cancelled : undefined;
  return {
    status_code: statusCode,
    stdout,
    stderr,
    success,
    cancelled,
  };
}

function parseNestedRecord(value: unknown): Record<string, unknown> | null {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function outputText(value: string | undefined): string {
  return value?.trim() ? value : "No output recorded.";
}

function statusCodeLabel(output: CommandOutput | null): string {
  if (!output) return "-";
  if (output.status_code == null) {
    return output.cancelled ? "terminated" : "-";
  }
  return String(output.status_code);
}

function latestRelatedEvent(
  step: ResearchJobStep,
  events: ResearchJobEvent[],
): ResearchJobEvent | null {
  const related = [...events]
    .filter((event) => event.step_id === step.id)
    .sort((a, b) => b.timestamp_ms - a.timestamp_ms);
  return (
    related.find((event) => outputFromRecord(parseRecord(event.details_json)) != null) ??
    related[0] ??
    null
  );
}

function activeRecoveryStep(
  job: ResearchJob,
  steps: ResearchJobStep[],
): ResearchJobStep | null {
  if (job.status === "blocked") {
    return steps.find((step) => step.status === "blocked") ?? null;
  }
  if (job.status === "failed") {
    return steps.find((step) => step.status === "failed") ?? null;
  }
  const expiredLease = steps.find(
    (step) =>
      (step.status === "running" || step.status === "leased") &&
      isStepLeaseExpired(step),
  );
  if (expiredLease) return expiredLease;
  return steps.find((step) => step.status === "retryable") ?? null;
}

function buildGuidance(
  step: ResearchJobStep,
  output: CommandOutput | null,
  staleLease: boolean,
): string[] {
  const stderr = output?.stderr ?? "";
  if (staleLease) {
    return [
      "The active step lease refresh is overdue. Confirm no worker command is still running before clearing the lease.",
      "Retry keeps the same inputs. Clone only if the inputs also need to change.",
    ];
  }
  const missingBoundaryPrice =
    step.name === "prepare_backtest_input" &&
    (stderr.includes("missing_open_prices") ||
      stderr.includes("missing_close_prices"));
  if (missingBoundaryPrice) {
    return [
      "The prepare step failed because the selected interval is missing boundary prices.",
      "Retry with unchanged inputs is likely to hit the same blocker. Prefer Clone with adjusted Start and End.",
    ];
  }
  if (step.status === "blocked") {
    return [
      "Resolve blocker records operator review and lets the job continue from the blocked step.",
      "Retry uses the same inputs and may repeat the same blocker if the underlying cause is deterministic.",
      "Clone is the safer path when params, artifact, or interval need edits.",
    ];
  }
  if (step.status === "failed" || step.status === "retryable") {
    return [
      "Retry is reasonable for transient worker or dependency failures.",
      "Clone with edited params is safer when command stderr points to bad inputs.",
    ];
  }
  return [
    "Review the active step and raw event details before choosing a recovery action.",
  ];
}

function recoveryContext(
  job: ResearchJob,
  steps: ResearchJobStep[],
  events: ResearchJobEvent[],
): RecoveryContext | null {
  const step = activeRecoveryStep(job, steps);
    if (!step) return null;
  const event = latestRelatedEvent(step, events);
  const details = parseRecord(event?.details_json ?? null) ?? parseRecord(step.output_json);
  const command = commandFromRecord(details);
  const output = outputFromRecord(details);
  const staleLease =
    (step.status === "running" || step.status === "leased") &&
    isStepLeaseExpired(step);
  return {
    step,
    event,
    details,
    command,
    output,
    staleLease,
    guidance: buildGuidance(step, output, staleLease),
  };
}

export function JobRecoveryDiagnosis({
  job,
  steps,
  events,
}: JobRecoveryDiagnosisProps) {
  const context = recoveryContext(job, steps, events);
  if (!context) return null;
  const { step, event, details, command, output, staleLease, guidance } = context;
  const tone = staleLease ? "warning" : step.status === "failed" ? "danger" : "info";
  const rawStepJson = JSON.stringify(step, null, 2);

  return (
    <SectionCard
      title="Recovery diagnosis"
      subtitle="Inspect the failed or blocked step before retrying or cloning."
    >
      <div className="space-y-3">
        <Banner
          tone={tone}
          title={staleLease ? "Lease refresh overdue" : "Operator recovery required"}
        >
          <div className="space-y-1">
            <div>
              Step {step.step_index + 1} ({humanize(step.name)}) is{" "}
              {humanize(step.status)}.
            </div>
            {step.error && <div>{step.error}</div>}
          </div>
        </Banner>
        <KeyValueList
          columns={2}
          items={[
            {
              label: "Step",
              value: (
                <span className="inline-flex items-center gap-2">
                  <span>{humanize(step.name)}</span>
                  <StatusChip
                    label={humanize(step.status)}
                    tone={stepTone(step.status)}
                    compact
                  />
                </span>
              ),
            },
            { label: "Attempt count", value: step.attempts },
            {
              label: "Started",
              value: step.started_at ? formatDateTime(step.started_at) : "-",
            },
            {
              label: "Completed",
              value: step.completed_at ? formatDateTime(step.completed_at) : "-",
            },
            {
              label: "Lease owner",
              value: step.lease_owner ?? "-",
            },
            {
              label: "Lease until",
              value: step.leased_until_ms ? (
                <span className="inline-flex flex-wrap items-center gap-2">
                  <RelativeTime epochMs={step.leased_until_ms} />
                  {staleLease && (
                    <StatusChip
                      label="refresh overdue"
                      tone="warning"
                      compact
                    />
                  )}
                </span>
              ) : (
                "-"
              ),
            },
            {
              label: "Command",
              value: command?.program ?? "-",
            },
            {
              label: "Command cwd",
              value: command?.cwd ?? "-",
            },
            {
              label: "Status code",
              value: statusCodeLabel(output),
            },
          ]}
        />
        {command?.args && command.args.length > 0 && (
          <div>
            <div className="mb-1 text-[12px] font-semibold">Command args</div>
            <pre className="max-h-40 overflow-auto border border-border bg-surface p-2 font-mono text-[11px] whitespace-pre-wrap">
              {command.args.join(" ")}
            </pre>
          </div>
        )}
        <div className="grid gap-3 md:grid-cols-2">
          <div>
            <div className="mb-1 text-[12px] font-semibold">Stdout</div>
            <pre className="max-h-48 overflow-auto border border-border bg-surface p-2 font-mono text-[11px] whitespace-pre-wrap">
              {outputText(output?.stdout)}
            </pre>
          </div>
          <div>
            <div className="mb-1 text-[12px] font-semibold">Stderr</div>
            <pre className="max-h-48 overflow-auto border border-border bg-surface p-2 font-mono text-[11px] whitespace-pre-wrap">
              {outputText(output?.stderr ?? step.error ?? undefined)}
            </pre>
          </div>
        </div>
        <div className="border border-border bg-bg px-3 py-2 text-[12px]">
          <div className="font-semibold">Recovery options</div>
          <ul className="mt-2 list-disc space-y-1 pl-5 text-muted">
            {guidance.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>
        {event && (
          <JsonViewer
            value={event.details_json}
            label="Raw event details"
            emptyLabel="No event details recorded"
            maxHeight={260}
          />
        )}
        <JsonViewer
          value={rawStepJson}
          label="Raw step state"
          emptyLabel="No step details recorded"
          maxHeight={260}
        />
        {details && !event && (
          <JsonViewer
            value={JSON.stringify(details)}
            label="Raw output details"
            emptyLabel="No output details recorded"
            maxHeight={260}
          />
        )}
      </div>
    </SectionCard>
  );
}
