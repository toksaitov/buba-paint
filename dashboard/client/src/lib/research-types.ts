import type {
  MachineHostIdentity,
  MachineSample,
  MachineSamplerHealth,
} from "./types";

export const TRANSFER_STALE_MS = 30 * 60 * 1000;

export type MachineRole = "live" | "research" | "controller";

export type MachineStatus =
  | "not_configured"
  | "configured"
  | "online"
  | "idle"
  | "busy"
  | "degraded"
  | "error"
  | "disabled"
  | "unreachable"
  | "maintenance";

export type ArtifactStatus = "available" | "archived";

export type TransferStatus =
  | "queued"
  | "running"
  | "retryable"
  | "paused"
  | "failed"
  | "cancelled"
  | "completed";

export type TransferChecksumStatus =
  | "pending"
  | "verifying"
  | "verified"
  | "failed"
  | "skipped";

export type JobType = "export" | "current_params" | "sweep";

export type JobStatus =
  | "queued"
  | "running"
  | "retryable"
  | "paused"
  | "blocked"
  | "failed"
  | "cancelled"
  | "completed";

export type StepStatus =
  | "queued"
  | "leased"
  | "running"
  | "retryable"
  | "blocked"
  | "paused"
  | "failed"
  | "cancelled"
  | "completed";

export type ReportStatus = "available" | "archived";

export type JobTemplateStatus = "active" | "archived";

export type EventLevel = "info" | "warn" | "error" | "progress" | "debug";

export interface ResearchMachine {
  id: string;
  name: string;
  role: MachineRole;
  ssh_alias: string | null;
  status: MachineStatus;
  details_json: string | null;
  created_at: number;
  updated_at: number;
}

export interface ResearchMachineDependencyCounts {
  artifacts: number;
  transfers_as_source: number;
  transfers_as_destination: number;
  active_transfers: number;
  jobs_using_source_artifacts: number;
  reports_using_source_artifacts: number;
}

export interface MachineHealthResponse {
  machine: ResearchMachine;
  details: Record<string, unknown> | null;
  dependencies: ResearchMachineDependencyCounts;
  disabled: boolean;
}

export interface ResearchMachineTelemetryState {
  machine_id: string;
  worker_id: string;
  worker_version: string | null;
  worker_status: string;
  host: MachineHostIdentity | null;
  sampler: MachineSamplerHealth | null;
  activity: Record<string, unknown> | null;
  last_heartbeat_ms: number;
  last_sample_ms: number | null;
  last_error: string | null;
  updated_at: number;
}

export interface MachineTelemetryResponse {
  machine: ResearchMachine;
  telemetry: ResearchMachineTelemetryState | null;
  samples: MachineSample[];
  dependencies: ResearchMachineDependencyCounts;
  disabled: boolean;
  stale: boolean;
  stale_after_ms: number;
}

export interface ResearchArtifact {
  id: string;
  source_machine_id: string | null;
  kind: string;
  status: ArtifactStatus;
  run_mode: string | null;
  artifact_root: string | null;
  manifest_path: string | null;
  bundle_path: string | null;
  source_db_path: string | null;
  interval_start_ms: number | null;
  interval_end_ms: number | null;
  bytes: number | null;
  checksum: string | null;
  replay_quality_class: string | null;
  backtest_ready_class: string | null;
  live_fidelity_class: string | null;
  created_at: number;
  updated_at: number;
  archived_at: number | null;
}

export interface ArtifactManifestFile {
  logical_name: string;
  kind: string;
  relative_path: string;
  bytes: number;
  sha256: string;
}

export interface ArtifactManifest {
  schema_version: number;
  artifact_id: string;
  kind: string;
  source_machine_id: string | null;
  run_mode: string | null;
  created_at_ms: number;
  interval_start_ms: number | null;
  interval_end_ms: number | null;
  files: ArtifactManifestFile[];
}

export interface ArtifactVerification {
  artifact_id: string;
  files_checked: number;
  bytes_checked: number;
}

export interface ImportArtifactResponse {
  artifact: ResearchArtifact;
  verification: ArtifactVerification;
}

export interface RegisterArtifactManifestSummary {
  artifact_id: string;
  files: number;
  bytes: number;
}

export interface RegisterArtifactResponse {
  artifact: ResearchArtifact;
  manifest_summary: RegisterArtifactManifestSummary;
}

export interface VerifyArtifactResponse {
  artifact: ResearchArtifact;
  verification: ArtifactVerification;
}

export interface ArtifactTransfer {
  id: string;
  artifact_id: string;
  source_machine_id: string | null;
  dest_machine_id: string | null;
  status: TransferStatus;
  bytes_total: number | null;
  bytes_done: number;
  checksum_status: TransferChecksumStatus | null;
  error: string | null;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
}

export interface VerifyTransferResponse {
  transfer: ArtifactTransfer;
  verification: ArtifactVerification;
}

export interface ResearchJob {
  id: string;
  job_type: JobType;
  artifact_id: string | null;
  status: JobStatus;
  priority: number;
  requested_by: string;
  params_json: string | null;
  created_at: number;
  updated_at: number;
  cancelled_at: number | null;
  completed_at: number | null;
}

export interface ResearchJobStep {
  id: string;
  job_id: string;
  step_index: number;
  name: string;
  status: StepStatus;
  lease_owner: string | null;
  leased_until_ms: number | null;
  attempts: number;
  input_json: string | null;
  output_json: string | null;
  error: string | null;
  created_at: number;
  updated_at: number;
  started_at: number | null;
  completed_at: number | null;
}

export interface ResearchJobEvent {
  id: string;
  job_id: string;
  step_id: string | null;
  timestamp_ms: number;
  level: EventLevel;
  message: string;
  details_json: string | null;
}

export interface JobDetailResponse {
  job: ResearchJob;
  steps: ResearchJobStep[];
  events: ResearchJobEvent[];
}

export interface ArchiveScratchSummary {
  deleted_paths: string[];
  skipped_paths: string[];
}

export interface ArchiveScratchResponse extends JobDetailResponse {
  report: ResearchReport;
  archive: ArchiveScratchSummary;
}

export interface ResearchReport {
  id: string;
  job_id: string;
  artifact_id: string | null;
  title: string;
  status: ReportStatus;
  summary_json: string | null;
  report_path: string | null;
  csv_path: string | null;
  created_at: number;
  updated_at: number;
}

export interface ResearchJobTemplate {
  id: string;
  name: string;
  description: string | null;
  job_type: "current_params" | "sweep";
  artifact_id: string | null;
  priority: number;
  params_json: string;
  status: JobTemplateStatus;
  created_by: string;
  created_at: number;
  updated_at: number;
  last_used_at: number | null;
  usage_count: number;
}

export interface UpsertJobTemplateRequest {
  name: string;
  description?: string;
  job_type: "current_params" | "sweep";
  artifact_id?: string;
  priority?: number;
  params: Record<string, unknown>;
}

export interface ResearchQueueJobItem {
  job: ResearchJob;
  step: ResearchJobStep | null;
  stale: boolean;
}

export interface ResearchQueueTransferItem {
  transfer: ArtifactTransfer;
  stale: boolean;
}

export interface ResearchQueueMachineItem {
  machine: ResearchMachine;
  dependencies: ResearchMachineDependencyCounts;
}

export interface ResearchRetentionTotals {
  jobs: number;
  reports: number;
  artifacts: number;
  scratch_bytes: number;
  report_bytes: number;
  artifact_bytes: number;
}

export interface ResearchQueueResponse {
  generated_at_ms: number;
  counts: {
    jobs_total: number;
    jobs_active: number;
    jobs_waiting: number;
    jobs_running: number;
    jobs_retryable: number;
    jobs_blocked: number;
    jobs_failed: number;
    jobs_completed: number;
    stale_leases: number;
    transfers_active: number;
    transfers_attention: number;
    disabled_hosts: number;
  };
  jobs: {
    running: ResearchQueueJobItem[];
    waiting: ResearchQueueJobItem[];
    retryable: ResearchQueueJobItem[];
    blocked: ResearchQueueJobItem[];
    failed: ResearchQueueJobItem[];
    stale_leases: ResearchQueueJobItem[];
  };
  transfers: {
    active: ResearchQueueTransferItem[];
    attention: ResearchQueueTransferItem[];
    stale: ResearchQueueTransferItem[];
  };
  disabled_hosts: ResearchQueueMachineItem[];
  recent_reports: ResearchReport[];
  retention: ResearchRetentionTotals;
}

export interface ResearchRetentionJobCandidate {
  job: ResearchJob;
  report: ResearchReport | null;
  scratch_bytes: number;
  eligible: boolean;
  skipped_reason: string | null;
}

export interface ResearchRetentionReportCandidate {
  report: ResearchReport;
  bytes: number;
  eligible: boolean;
  skipped_reason: string | null;
}

export interface ResearchRetentionArtifactCandidate {
  artifact: ResearchArtifact;
  bytes: number;
  active_dependency_count: number;
  eligible: boolean;
  skipped_reason: string | null;
}

export interface ResearchRetentionResponse {
  generated_at_ms: number;
  jobs: ResearchRetentionJobCandidate[];
  reports: ResearchRetentionReportCandidate[];
  artifacts: ResearchRetentionArtifactCandidate[];
  totals: ResearchRetentionTotals;
}

export interface RetentionArchiveRequest {
  job_ids?: string[];
  report_ids?: string[];
  artifact_ids?: string[];
}

export interface RetentionArchiveJobResult {
  id: string;
  status: "archived" | "skipped" | "error";
  job: ResearchJob | null;
  report: ResearchReport | null;
  archive: ArchiveScratchSummary | null;
  message: string | null;
}

export interface RetentionArchiveMetadataResult<T> {
  id: string;
  status: "archived" | "skipped" | "error";
  item: T | null;
  message: string | null;
}

export interface RetentionArchiveResponse {
  jobs: RetentionArchiveJobResult[];
  reports: RetentionArchiveMetadataResult<ResearchReport>[];
  artifacts: RetentionArchiveMetadataResult<ResearchArtifact>[];
  totals: ResearchRetentionTotals;
}

export interface RegenerateReportResponse {
  report: ResearchReport;
  report_path: string;
  csv_path: string;
}

export interface CreateMachineRequest {
  id: string;
  name: string;
  role: MachineRole;
  ssh_alias?: string;
  status?: MachineStatus;
  details?: Record<string, unknown>;
}

export interface UpdateMachineRequest {
  name?: string;
  role?: MachineRole;
  ssh_alias?: string | null;
  status?: MachineStatus;
  details?: Record<string, unknown> | null;
}

export interface ImportArtifactRequest {
  artifact_root: string;
  artifact_id?: string;
  source_machine_id?: string;
  status?: ArtifactStatus;
}

export interface RegisterArtifactRequest {
  artifact_root: string;
  manifest: ArtifactManifest;
  source_machine_id?: string;
  status?: ArtifactStatus;
}

export interface UpdateArtifactRequest {
  source_machine_id?: string;
  run_mode?: string;
  replay_quality_class?: string;
  backtest_ready_class?: string;
  live_fidelity_class?: string;
}

export interface CreateTransferRequest {
  artifact_id: string;
  source_machine_id?: string;
  dest_machine_id?: string;
  bytes_total?: number;
}

export interface TransferProgressRequest {
  status: TransferStatus;
  bytes_done?: number;
  bytes_total?: number;
  checksum_status?: TransferChecksumStatus;
  error?: string;
}

export interface RetryTransferRequest {
  resume?: boolean;
}

export interface CreateJobRequest {
  job_type: JobType;
  artifact_id?: string;
  priority?: number;
  params?: Record<string, unknown>;
  template_id?: string;
}

export interface UpdateJobRequest {
  artifact_id?: string | null;
  priority?: number;
  params?: Record<string, unknown> | null;
}

export interface CloneJobRequest {
  artifact_id?: string | null;
  priority?: number;
  params?: Record<string, unknown> | null;
}

export interface AppendEventRequest {
  step_id?: string;
  level: EventLevel;
  message: string;
  details?: Record<string, unknown>;
}

export interface UpdateReportRequest {
  title?: string;
  status?: ReportStatus;
}

export type ResearchEntityKind =
  | "machine"
  | "artifact"
  | "transfer"
  | "job"
  | "step"
  | "report";

export type ResearchAction =
  | "read"
  | "create"
  | "update"
  | "delete"
  | "delete_with_files"
  | "archive"
  | "restore"
  | "cancel"
  | "pause"
  | "resume"
  | "continue"
  | "retry"
  | "clone"
  | "verify"
  | "health"
  | "enable"
  | "disable"
  | "import"
  | "register"
  | "clear_lease"
  | "resolve_blocker"
  | "append_event"
  | "regenerate_report"
  | "archive_scratch";
