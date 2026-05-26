import type {
  ArtifactStatus,
  JobStatus,
  MachineStatus,
  ReportStatus,
  ResearchAction,
  ResearchEntityKind,
  ResearchJobStep,
  StepStatus,
  TransferStatus,
} from "./research-types";

export interface ActionContext {
  leased_until_ms?: number | null;
  has_reports?: boolean;
  is_default_machine?: boolean;
  has_dependencies?: boolean;
  is_custom?: boolean;
  now_ms?: number;
}

const OBSERVER_ALLOWED: ResearchAction[] = ["read", "health"];

export function canPerform(
  role: "admin" | "observer",
  action: ResearchAction,
): boolean {
  if (role === "admin") return true;
  return OBSERVER_ALLOWED.includes(action);
}

export type ActionGateState = "enabled" | "disabled_hint";

export function getActionGateState(
  role: "admin" | "observer" | undefined,
  action: ResearchAction,
): ActionGateState {
  if (role && canPerform(role, action)) return "enabled";
  return "disabled_hint";
}

export const JOB_TERMINAL_STATUSES: JobStatus[] = [
  "completed",
  "failed",
  "cancelled",
];

export const TRANSFER_TERMINAL_STATUSES: TransferStatus[] = [
  "completed",
  "cancelled",
  "failed",
];

export const TRANSFER_DELETABLE_STATUSES: TransferStatus[] = [
  "completed",
  "cancelled",
  "failed",
];

export function isJobTerminal(status: JobStatus): boolean {
  return JOB_TERMINAL_STATUSES.includes(status);
}

export function isTransferTerminal(status: TransferStatus): boolean {
  return TRANSFER_TERMINAL_STATUSES.includes(status);
}

export function isStepLeaseExpired(
  step: Pick<ResearchJobStep, "leased_until_ms">,
  nowMs: number = Date.now(),
): boolean {
  return step.leased_until_ms != null && step.leased_until_ms <= nowMs;
}

export type ChipTone = "neutral" | "muted" | "success" | "warning" | "danger";

export function machineTone(status: MachineStatus): ChipTone {
  switch (status) {
    case "online":
    case "idle":
    case "configured":
      return "success";
    case "busy":
      return "warning";
    case "degraded":
    case "unreachable":
    case "maintenance":
      return "warning";
    case "error":
      return "danger";
    case "disabled":
      return "muted";
    case "not_configured":
    default:
      return "neutral";
  }
}

export function artifactTone(status: ArtifactStatus): ChipTone {
  return status === "archived" ? "muted" : "success";
}

export function transferTone(status: TransferStatus): ChipTone {
  switch (status) {
    case "completed":
      return "success";
    case "running":
      return "warning";
    case "retryable":
    case "failed":
      return "danger";
    case "paused":
      return "muted";
    case "cancelled":
      return "muted";
    case "queued":
    default:
      return "neutral";
  }
}

export function checksumTone(
  status: string | null | undefined,
): ChipTone {
  switch (status) {
    case "verified":
      return "success";
    case "verifying":
    case "pending":
      return "neutral";
    case "failed":
      return "danger";
    case "skipped":
      return "muted";
    default:
      return "muted";
  }
}

export function jobTone(status: JobStatus): ChipTone {
  switch (status) {
    case "completed":
      return "success";
    case "running":
      return "warning";
    case "failed":
    case "blocked":
      return "danger";
    case "retryable":
      return "warning";
    case "paused":
    case "cancelled":
      return "muted";
    case "queued":
    default:
      return "neutral";
  }
}

export function stepTone(status: StepStatus): ChipTone {
  switch (status) {
    case "completed":
      return "success";
    case "running":
    case "leased":
      return "warning";
    case "blocked":
    case "failed":
      return "danger";
    case "retryable":
      return "warning";
    case "paused":
    case "cancelled":
      return "muted";
    case "queued":
    default:
      return "neutral";
  }
}

export function reportTone(status: ReportStatus): ChipTone {
  return status === "archived" ? "muted" : "success";
}

export function jobTypeTone(): ChipTone {
  return "neutral";
}

export function jobTypeLabel(type: string): string {
  switch (type) {
    case "export":
      return "Export";
    case "current_params":
      return "Backtest";
    case "sweep":
      return "Sweep";
    default:
      return type;
  }
}

export type ProgressTone = "neutral" | "warning" | "danger";

export function progressTone(tone: ChipTone): ProgressTone | undefined {
  if (tone === "warning") return "warning";
  if (tone === "danger") return "danger";
  if (tone === "neutral") return "neutral";
  return undefined;
}

const ALL_ACTIONS: ResearchAction[] = [
  "read",
  "create",
  "update",
  "delete",
  "delete_with_files",
  "archive",
  "restore",
  "cancel",
  "pause",
  "resume",
  "continue",
  "retry",
  "clone",
  "verify",
  "health",
  "enable",
  "disable",
  "import",
  "register",
  "clear_lease",
  "resolve_blocker",
  "append_event",
  "regenerate_report",
];

export function getAllowedActions(
  entity: ResearchEntityKind,
  status: string,
  ctx: ActionContext = {},
): ResearchAction[] {
  switch (entity) {
    case "job":
      return jobActions(status as JobStatus, ctx);
    case "step":
      return stepActions(status as StepStatus, ctx);
    case "transfer":
      return transferActions(status as TransferStatus);
    case "artifact":
      return artifactActions(status as ArtifactStatus);
    case "machine":
      return machineActions(status as MachineStatus, ctx);
    case "report":
      return reportActions(status as ReportStatus);
    default:
      return [];
  }
}

function jobActions(status: JobStatus, ctx: ActionContext): ResearchAction[] {
  const deletable = !ctx.has_reports;
  switch (status) {
    case "queued":
      return ["update", "cancel", "pause", "append_event"];
    case "running":
    case "leased" as JobStatus:
      return ["cancel", "append_event"];
    case "retryable":
      return ["retry", "cancel", "append_event"];
    case "paused":
      return ["resume", "cancel", "clone", "append_event"];
    case "blocked":
      return ["retry", "continue", "cancel", "clone", "append_event"];
    case "failed":
      return [
        "retry",
        "clone",
        ...((deletable ? ["delete"] : []) as ResearchAction[]),
        "append_event",
      ];
    case "cancelled":
      return [
        "continue",
        "clone",
        ...((deletable ? ["delete"] : []) as ResearchAction[]),
        "append_event",
      ];
    case "completed":
      return [
        "clone",
        "regenerate_report",
        ...((deletable ? ["delete"] : []) as ResearchAction[]),
        "append_event",
      ];
    default:
      return ["append_event"];
  }
}

function stepActions(
  status: StepStatus,
  ctx: ActionContext,
): ResearchAction[] {
  const now = ctx.now_ms ?? Date.now();
  switch (status) {
    case "queued":
      return ["cancel"];
    case "leased": {
      const expired =
        ctx.leased_until_ms != null && ctx.leased_until_ms <= now;
      return expired ? ["cancel", "clear_lease"] : ["cancel"];
    }
    case "running":
      return ["cancel"];
    case "retryable":
    case "paused":
      return ["retry", "cancel"];
    case "blocked":
      return ["retry", "cancel", "resolve_blocker"];
    case "failed":
      return ["retry", "cancel"];
    case "cancelled":
      return ["retry"];
    case "completed":
    default:
      return [];
  }
}

function transferActions(status: TransferStatus): ResearchAction[] {
  switch (status) {
    case "queued":
      return ["cancel", "pause"];
    case "running":
      return ["cancel", "pause"];
    case "retryable":
      return ["retry", "cancel"];
    case "paused":
      return ["resume", "cancel"];
    case "failed":
      return ["retry", "cancel", "delete"];
    case "cancelled":
      return ["retry", "delete"];
    case "completed":
      return ["verify", "delete"];
    default:
      return [];
  }
}

function artifactActions(status: ArtifactStatus): ResearchAction[] {
  if (status === "archived") {
    return ["restore", "delete", "delete_with_files"];
  }
  return ["update", "verify", "archive", "delete", "delete_with_files"];
}

function machineActions(
  status: MachineStatus,
  ctx: ActionContext,
): ResearchAction[] {
  const canDelete = !ctx.is_default_machine && !ctx.has_dependencies;
  if (status === "disabled") {
    return [
      "update",
      "enable",
      "health",
      ...((canDelete ? ["delete"] : []) as ResearchAction[]),
    ];
  }
  return [
    "update",
    "disable",
    "health",
    ...((canDelete ? ["delete"] : []) as ResearchAction[]),
  ];
}

function reportActions(status: ReportStatus): ResearchAction[] {
  if (status === "archived") {
    return ["restore", "delete", "delete_with_files"];
  }
  return ["update", "archive", "delete", "delete_with_files"];
}

export const ACTION_LABELS: Record<ResearchAction, string> = {
  read: "View",
  create: "Create",
  update: "Edit",
  delete: "Delete record",
  delete_with_files: "Delete with files",
  archive: "Archive",
  restore: "Restore",
  cancel: "Cancel",
  pause: "Pause",
  resume: "Resume",
  continue: "Continue",
  retry: "Retry",
  clone: "Clone",
  verify: "Verify",
  health: "Health",
  enable: "Enable",
  disable: "Disable",
  import: "Import",
  register: "Register",
  clear_lease: "Clear stale lease",
  resolve_blocker: "Resolve blocker",
  append_event: "Add note",
  regenerate_report: "Regenerate report",
};

export function actionLabel(action: ResearchAction): string {
  return ACTION_LABELS[action] ?? action;
}

export function permissionHint(action: ResearchAction): string {
  if (OBSERVER_ALLOWED.includes(action)) return "";
  return "Admin role required.";
}

export const RESEARCH_ACTIONS = ALL_ACTIONS;
