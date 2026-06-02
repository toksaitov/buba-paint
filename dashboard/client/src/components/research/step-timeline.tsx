import { useEffect, useState } from "react";
import {
  Banner,
  Button,
  InfoHint,
  RelativeTime,
  StatusChip,
} from "../ui/dashboard-primitives";
import { JsonViewer } from "./json-viewer";
import {
  actionLabel,
  getAllowedActions,
  getActionGateState,
  isStepLeaseExpired,
  permissionHint,
  stepTone,
} from "../../lib/research-permissions";
import type {
  ResearchAction,
  ResearchJobStep,
} from "../../lib/research-types";
import { cn, formatDurationShort, humanize } from "../../lib/utils";

const STEP_LABELS: Record<string, string> = {
  plan_export: "Plan export",
  snapshot_or_copy_runtime: "Snapshot runtime",
  write_artifact_manifest: "Write manifest",
  verify_artifact: "Verify artifact",
  validate_replay_data: "Validate replay",
  validate_backtest_input: "Validate backtest input",
  prepare_backtest_input: "Prepare backtest DB",
  run_backtest: "Run backtest",
  run_sweep: "Run sweep",
  write_report: "Write report",
};

interface StepActionHandler {
  (stepId: string, action: ResearchAction): void;
}

interface StepTimelineProps {
  steps: ResearchJobStep[];
  role: "admin" | "observer" | undefined;
  pendingStepAction?: { stepId: string; action: ResearchAction } | null;
  onAction?: StepActionHandler;
  nowMs?: number;
}

function humanStepName(name: string): string {
  return STEP_LABELS[name] ?? name.replace(/_/g, " ");
}

function leaseRefreshText(deadlineMs: number, nowMs: number): string {
  const delta = deadlineMs - nowMs;
  if (delta > 1_000) {
    return `refresh due in ${formatDurationShort(delta)}`;
  }
  if (delta >= -1_000) {
    return "refresh due now";
  }
  return `refresh overdue by ${formatDurationShort(-delta)}`;
}

export function StepTimeline({
  steps,
  role,
  pendingStepAction,
  onAction,
  nowMs,
}: StepTimelineProps) {
  const [tick, setTick] = useState<number>(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setTick(Date.now()), 5_000);
    return () => window.clearInterval(id);
  }, []);
  const effectiveNow = nowMs ?? tick;
  const [expanded, setExpanded] = useState<string | null>(null);

  if (steps.length === 0) {
    return (
      <div className="text-[12px] text-muted">No steps recorded yet.</div>
    );
  }

  return (
    <ol className="space-y-2">
      {steps.map((step) => {
        const isOpen = expanded === step.id;
        const tone = stepTone(step.status);
        const allowed = getAllowedActions("step", step.status, {
          leased_until_ms: step.leased_until_ms,
          now_ms: effectiveNow,
        });
        const leaseExpired = isStepLeaseExpired(step, effectiveNow);
        return (
          <li
            key={step.id}
            className={cn(
              "border border-border bg-bg",
              tone === "danger" && "border-accent-red",
              tone === "warning" && "border-accent-blue",
            )}
          >
            <button
              type="button"
              onClick={() => setExpanded(isOpen ? null : step.id)}
              className="flex w-full items-start justify-between gap-2 px-3 py-2 text-left"
              aria-expanded={isOpen}
            >
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[11px] tabular-nums text-muted">
                    {String(step.step_index + 1).padStart(2, "0")}
                  </span>
                  <span className="text-sm font-semibold">
                    {humanStepName(step.name)}
                  </span>
                  <StatusChip
                    label={humanize(step.status)}
                    tone={tone}
                    compact
                    dot={tone === "warning"}
                  />
                  {step.attempts > 1 && (
                    <span className="text-[11px] text-muted">
                      attempt {step.attempts}
                    </span>
                  )}
                </div>
                {step.error && (
                  <div className="line-clamp-1 text-[12px] text-accent-red">
                    {step.error}
                  </div>
                )}
                {step.lease_owner && (
                  <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted">
                    <span>Leased to {step.lease_owner}</span>
                    {step.leased_until_ms != null && (
                      <>
                        <span>{leaseRefreshText(step.leased_until_ms, effectiveNow)}</span>
                        {leaseExpired && (
                          <StatusChip
                            label="refresh overdue"
                            tone="warning"
                            compact
                          />
                        )}
                      </>
                    )}
                  </div>
                )}
              </div>
              <span className="text-[11px] text-muted">
                {isOpen ? "Hide" : "Details"}
              </span>
            </button>
            {isOpen && (
              <div className="space-y-3 border-t border-border px-3 py-3">
                {step.error && (
                  <Banner tone="danger" title="Step error">
                    <pre className="whitespace-pre-wrap font-mono text-[11px]">
                      {step.error}
                    </pre>
                  </Banner>
                )}
                <div className="grid gap-3 sm:grid-cols-2">
                  <JsonViewer
                    value={step.input_json}
                    label="Input"
                    emptyLabel="No input recorded"
                    maxHeight={220}
                  />
                  <JsonViewer
                    value={step.output_json}
                    label="Output"
                    emptyLabel="No output yet"
                    maxHeight={220}
                  />
                </div>
                {(step.started_at || step.completed_at) && (
                  <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted">
                    {step.started_at && (
                      <span>
                        Started{" "}
                        <RelativeTime epochMs={step.started_at} />
                      </span>
                    )}
                    {step.completed_at && (
                      <span>
                        Finished{" "}
                        <RelativeTime epochMs={step.completed_at} />
                      </span>
                    )}
                  </div>
                )}
                {role && onAction && (
                  <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
                    {allowed.map((action) => {
                      const gate = getActionGateState(role, action);
                      const disabled = gate !== "enabled";
                      const isPending =
                        pendingStepAction?.stepId === step.id &&
                        pendingStepAction?.action === action;
                      return (
                        <Button
                          key={action}
                          size="sm"
                          tone={action === "cancel" ? "danger" : "neutral"}
                          disabled={disabled || isPending}
                          state={isPending ? "pending" : "idle"}
                          onClick={() => onAction(step.id, action)}
                          title={disabled ? permissionHint(action) : undefined}
                        >
                          {actionLabel(action)}
                        </Button>
                      );
                    })}
                    <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
                      <InfoHint
                        label="Skip step"
                        text="Skip step is not yet supported by the backend."
                      />
                      Skip step: unsupported
                    </span>
                  </div>
                )}
              </div>
            )}
          </li>
        );
      })}
    </ol>
  );
}
