import type {
  ArtifactTransfer,
  JobDetailResponse,
  ResearchArtifact,
  ResearchJob,
  ResearchJobEvent,
  ResearchJobStep,
  ResearchMachine,
  ResearchMachineTelemetryState,
  ResearchReport,
  MachineTelemetryResponse,
} from "./research-types";
import type { MachineSample } from "./types";

export const FIXTURE_TIMESTAMP_MS = 1_779_000_000_000;
export const FIXTURE_INTERVAL_START_MS = 1_779_000_000_000;
export const FIXTURE_INTERVAL_END_MS = 1_779_000_600_000;
export const FIXTURE_USER_ID = "fixture-user-researcher";

const FIXTURE_PAYLOAD = "fixture artifact runtime database\n";
const FIXTURE_BYTES = FIXTURE_PAYLOAD.length;
const FIXTURE_CHECKSUM =
  "e7d8e9b5d6f4c3b2a190e8f7d6c5b4a3928171615141312110f0e0d0c0b0a090";

export function fixtureMachineLive(): ResearchMachine {
  return {
    id: "fixture-live",
    name: "Fixture Live Source",
    role: "live",
    ssh_alias: "fixture-live",
    status: "configured",
    details_json: JSON.stringify({
      host: "fixture-live",
      purpose: "ui-source",
    }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureMachineResearch(): ResearchMachine {
  return {
    id: "fixture-research",
    name: "Fixture Research Worker",
    role: "research",
    ssh_alias: "fixture-research",
    status: "idle",
    details_json: JSON.stringify({
      heartbeat_status: "idle",
      queue_depth: 0,
      worker_id: "fixture-worker",
    }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureMachineDisabled(): ResearchMachine {
  return {
    id: "fixture-disabled",
    name: "Fixture Disabled Worker",
    role: "research",
    ssh_alias: "fixture-disabled",
    status: "disabled",
    details_json: JSON.stringify({ reason: "operator disabled" }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureMachineDefaultLive(): ResearchMachine {
  return {
    id: "live",
    name: "Buba Paint Live",
    role: "live",
    ssh_alias: "buba-paint",
    status: "configured",
    details_json: JSON.stringify({ host: "buba-paint", phase: "local_first" }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureMachineDefaultResearch(): ResearchMachine {
  return {
    id: "research",
    name: "Research Worker",
    role: "research",
    ssh_alias: "testing",
    status: "not_configured",
    details_json: JSON.stringify({
      host: "testing",
      deferred_until_phase: 7,
    }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureMachineSample(
  overrides: Partial<MachineSample> = {},
): MachineSample {
  return {
    sampled_at_ms: FIXTURE_TIMESTAMP_MS,
    cpu_percent: 25,
    per_core_cpu: [22, 28],
    load_one: 0.7,
    load_five: 0.6,
    load_fifteen: 0.5,
    mem_used_bytes: 4 * 1024 * 1024 * 1024,
    mem_total_bytes: 16 * 1024 * 1024 * 1024,
    mem_available_bytes: 12 * 1024 * 1024 * 1024,
    swap_used_bytes: 0,
    swap_total_bytes: 2 * 1024 * 1024 * 1024,
    disk_used_bytes: 100 * 1024 * 1024 * 1024,
    disk_total_bytes: 500 * 1024 * 1024 * 1024,
    disk_mount: "/research",
    ...overrides,
  };
}

export function fixtureMachineTelemetryState(
  overrides: Partial<ResearchMachineTelemetryState> = {},
): ResearchMachineTelemetryState {
  return {
    machine_id: "fixture-research",
    worker_id: "fixture-worker",
    worker_version: "0.1.0",
    worker_status: "idle",
    host: {
      hostname: "fixture-research",
      os_name: "Linux",
      os_version: "fixture",
      kernel_version: "fixture-kernel",
      cpu_count: 2,
      total_ram_bytes: 16 * 1024 * 1024 * 1024,
    },
    sampler: {
      sample_interval_ms: 5_000,
      samples_collected: 12,
      last_error: null,
    },
    activity: {
      phase: "idle",
      heartbeat_interval_ms: 30_000,
      processed_last_tick: 0,
      transfers_processed_last_tick: 0,
    },
    last_heartbeat_ms: FIXTURE_TIMESTAMP_MS,
    last_sample_ms: FIXTURE_TIMESTAMP_MS,
    last_error: null,
    updated_at: FIXTURE_TIMESTAMP_MS,
    ...overrides,
  };
}

export function fixtureMachineTelemetryResponse(
  overrides: Partial<MachineTelemetryResponse> = {},
): MachineTelemetryResponse {
  const state = fixtureMachineTelemetryState();
  const sample = fixtureMachineSample();
  return {
    machine: fixtureMachineResearch(),
    telemetry: state,
    samples: [sample],
    dependencies: {
      artifacts: 1,
      transfers_as_source: 0,
      transfers_as_destination: 2,
      active_transfers: 1,
      jobs_using_source_artifacts: 3,
      reports_using_source_artifacts: 1,
    },
    disabled: false,
    stale: false,
    stale_after_ms: 90_000,
    ...overrides,
  };
}

function baseArtifact(): Omit<
  ResearchArtifact,
  "id" | "status" | "archived_at"
> {
  return {
    source_machine_id: "fixture-live",
    kind: "readonly_run",
    run_mode: "live_readonly",
    artifact_root: "/research/artifacts/fixture-artifact-available",
    manifest_path: "/research/artifacts/fixture-artifact-available/manifest.json",
    bundle_path: null,
    source_db_path: "/runtime/paint.db",
    interval_start_ms: FIXTURE_INTERVAL_START_MS,
    interval_end_ms: FIXTURE_INTERVAL_END_MS,
    bytes: FIXTURE_BYTES,
    checksum: FIXTURE_CHECKSUM,
    replay_quality_class: "sweep_grade",
    backtest_ready_class: "backtest_ready",
    live_fidelity_class: "not_checked",
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureArtifactAvailable(): ResearchArtifact {
  return {
    ...baseArtifact(),
    id: "fixture-artifact-available",
    status: "available",
    archived_at: null,
  };
}

export function fixtureArtifactArchived(): ResearchArtifact {
  return {
    ...baseArtifact(),
    id: "fixture-artifact-archived",
    artifact_root: "/research/artifacts/fixture-artifact-archived",
    manifest_path: "/research/artifacts/fixture-artifact-archived/manifest.json",
    status: "archived",
    archived_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureArtifactBadChecksum(): ResearchArtifact {
  return {
    ...baseArtifact(),
    id: "fixture-artifact-bad-checksum",
    artifact_root: "/research/artifacts/fixture-artifact-bad-checksum",
    manifest_path: "/research/artifacts/fixture-artifact-bad-checksum/manifest.json",
    status: "available",
    archived_at: null,
  };
}

function baseTransfer(): Omit<
  ArtifactTransfer,
  | "id"
  | "status"
  | "bytes_done"
  | "checksum_status"
  | "error"
  | "completed_at"
> {
  return {
    artifact_id: "fixture-artifact-available",
    source_machine_id: "fixture-live",
    dest_machine_id: "fixture-research",
    bytes_total: 1000,
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureTransferRunning(): ArtifactTransfer {
  return {
    ...baseTransfer(),
    id: "fixture-transfer-running",
    status: "running",
    bytes_done: 400,
    checksum_status: "pending",
    error: null,
    completed_at: null,
  };
}

export function fixtureTransferRetryable(): ArtifactTransfer {
  return {
    ...baseTransfer(),
    id: "fixture-transfer-retryable",
    status: "retryable",
    bytes_done: 700,
    checksum_status: "failed",
    error: "network reset",
    completed_at: null,
  };
}

export function fixtureTransferPaused(): ArtifactTransfer {
  return {
    ...baseTransfer(),
    id: "fixture-transfer-paused",
    status: "paused",
    bytes_done: 250,
    checksum_status: "pending",
    error: null,
    completed_at: null,
  };
}

export function fixtureTransferCompleted(): ArtifactTransfer {
  return {
    ...baseTransfer(),
    id: "fixture-transfer-completed",
    status: "completed",
    bytes_done: 1000,
    checksum_status: "verified",
    error: null,
    completed_at: FIXTURE_TIMESTAMP_MS,
  };
}

interface StepBuildOpts {
  status: ResearchJobStep["status"];
  error?: string | null;
  output?: Record<string, unknown> | null;
  lease?: { owner: string; until_ms: number } | null;
  started?: boolean;
  completed?: boolean;
}

function buildStep(
  jobId: string,
  index: number,
  name: string,
  opts: StepBuildOpts,
): ResearchJobStep {
  const attempts = opts.status === "queued" ? 0 : 1;
  return {
    id: `${jobId}-step-${index}`,
    job_id: jobId,
    step_index: index,
    name,
    status: opts.status,
    lease_owner: opts.lease?.owner ?? null,
    leased_until_ms: opts.lease?.until_ms ?? null,
    attempts,
    input_json: null,
    output_json: opts.output ? JSON.stringify(opts.output) : null,
    error: opts.error ?? null,
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
    started_at: opts.started ? FIXTURE_TIMESTAMP_MS : null,
    completed_at: opts.completed ? FIXTURE_TIMESTAMP_MS : null,
  };
}

function buildEvent(
  jobId: string,
  level: "info" | "warn",
  status: string,
): ResearchJobEvent {
  return {
    id: `${jobId}-event-0`,
    job_id: jobId,
    step_id: null,
    timestamp_ms: FIXTURE_TIMESTAMP_MS,
    level,
    message: `fixture job is ${status}`,
    details_json: JSON.stringify({ fixture: true, status }),
  };
}

function buildJob(
  id: string,
  type: ResearchJob["job_type"],
  status: ResearchJob["status"],
  artifactId: string | null,
): ResearchJob {
  return {
    id,
    job_type: type,
    artifact_id: artifactId,
    status,
    priority: 0,
    requested_by: FIXTURE_USER_ID,
    params_json: artifactId
      ? JSON.stringify({ artifact_id: artifactId, balance: 200 })
      : JSON.stringify({ source_db_path: "/runtime/paint.db", dry_run: true }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
    cancelled_at: status === "cancelled" ? FIXTURE_TIMESTAMP_MS : null,
    completed_at: status === "completed" ? FIXTURE_TIMESTAMP_MS : null,
  };
}

const COMPLETED_OUT = (name: string) => ({
  fixture_step: name,
  status: "completed",
});

export function fixtureJobCompleted(): JobDetailResponse {
  const jobId = "fixture-job-completed";
  const steps = [
    "verify_artifact",
    "validate_replay_data",
    "validate_backtest_input",
    "prepare_backtest_input",
    "run_backtest",
    "write_report",
  ].map((name, idx) =>
    buildStep(jobId, idx, name, {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT(name),
    }),
  );
  return {
    job: buildJob(jobId, "current_params", "completed", "fixture-artifact-available"),
    steps,
    events: [buildEvent(jobId, "info", "completed")],
  };
}

export function fixtureJobBlocked(): JobDetailResponse {
  const jobId = "fixture-job-blocked";
  const steps: ResearchJobStep[] = [
    buildStep(jobId, 0, "verify_artifact", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("verify_artifact"),
    }),
    buildStep(jobId, 1, "validate_replay_data", {
      status: "blocked",
      started: true,
      error: "validation requires operator review",
    }),
    buildStep(jobId, 2, "validate_backtest_input", { status: "queued" }),
    buildStep(jobId, 3, "prepare_backtest_input", { status: "queued" }),
    buildStep(jobId, 4, "run_backtest", { status: "queued" }),
    buildStep(jobId, 5, "write_report", { status: "queued" }),
  ];
  return {
    job: buildJob(jobId, "current_params", "blocked", "fixture-artifact-available"),
    steps,
    events: [buildEvent(jobId, "warn", "blocked")],
  };
}

export function fixtureJobFailed(): JobDetailResponse {
  const jobId = "fixture-job-failed";
  const steps: ResearchJobStep[] = [
    buildStep(jobId, 0, "verify_artifact", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("verify_artifact"),
    }),
    buildStep(jobId, 1, "validate_replay_data", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("validate_replay_data"),
    }),
    buildStep(jobId, 2, "validate_backtest_input", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("validate_backtest_input"),
    }),
    buildStep(jobId, 3, "prepare_backtest_input", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("prepare_backtest_input"),
    }),
    buildStep(jobId, 4, "run_sweep", {
      status: "failed",
      started: true,
      error: "backtest command exited 1",
    }),
    buildStep(jobId, 5, "write_report", { status: "queued" }),
  ];
  return {
    job: buildJob(jobId, "sweep", "failed", "fixture-artifact-available"),
    steps,
    events: [buildEvent(jobId, "warn", "failed")],
  };
}

export function fixtureJobCancelled(): JobDetailResponse {
  const jobId = "fixture-job-cancelled";
  const steps: ResearchJobStep[] = [
    buildStep(jobId, 0, "plan_export", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("plan_export"),
    }),
    buildStep(jobId, 1, "snapshot_or_copy_runtime", {
      status: "cancelled",
      started: true,
      error: "operator cancelled",
    }),
    buildStep(jobId, 2, "write_artifact_manifest", { status: "queued" }),
    buildStep(jobId, 3, "verify_artifact", { status: "queued" }),
  ];
  return {
    job: buildJob(jobId, "export", "cancelled", null),
    steps,
    events: [buildEvent(jobId, "info", "cancelled")],
  };
}

export function fixtureJobRunning(): JobDetailResponse {
  const jobId = "fixture-job-running";
  const steps: ResearchJobStep[] = [
    buildStep(jobId, 0, "verify_artifact", {
      status: "running",
      started: true,
      lease: { owner: "fixture-worker", until_ms: FIXTURE_TIMESTAMP_MS + 300_000 },
    }),
    buildStep(jobId, 1, "validate_replay_data", { status: "queued" }),
    buildStep(jobId, 2, "validate_backtest_input", { status: "queued" }),
    buildStep(jobId, 3, "prepare_backtest_input", { status: "queued" }),
    buildStep(jobId, 4, "run_backtest", { status: "queued" }),
    buildStep(jobId, 5, "write_report", { status: "queued" }),
  ];
  return {
    job: buildJob(jobId, "current_params", "running", "fixture-artifact-available"),
    steps,
    events: [buildEvent(jobId, "info", "running")],
  };
}

export function fixtureJobPaused(): JobDetailResponse {
  const jobId = "fixture-job-paused";
  const steps: ResearchJobStep[] = [
    buildStep(jobId, 0, "verify_artifact", {
      status: "completed",
      started: true,
      completed: true,
      output: COMPLETED_OUT("verify_artifact"),
    }),
    buildStep(jobId, 1, "validate_replay_data", {
      status: "paused",
      started: true,
    }),
    buildStep(jobId, 2, "validate_backtest_input", { status: "queued" }),
    buildStep(jobId, 3, "prepare_backtest_input", { status: "queued" }),
    buildStep(jobId, 4, "run_backtest", { status: "queued" }),
    buildStep(jobId, 5, "write_report", { status: "queued" }),
  ];
  return {
    job: buildJob(jobId, "current_params", "paused", "fixture-artifact-available"),
    steps,
    events: [buildEvent(jobId, "info", "paused")],
  };
}

function baseReport(): Omit<
  ResearchReport,
  "id" | "job_id" | "title" | "status" | "report_path" | "csv_path"
> {
  return {
    artifact_id: "fixture-artifact-available",
    summary_json: JSON.stringify({
      fixture: true,
      net_pnl: 284.25,
      max_drawdown: -91.4,
      win_rate: 0.58,
      trade_count: 43,
    }),
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
  };
}

export function fixtureReportAvailable(): ResearchReport {
  return {
    ...baseReport(),
    id: "fixture-report-available",
    job_id: "fixture-job-completed",
    title: "Fixture Report Available",
    status: "available",
    report_path:
      "/research/jobs/fixture-report-available/fixture-report-available.json",
    csv_path:
      "/research/jobs/fixture-report-available/fixture-report-available.csv",
  };
}

export function fixtureReportArchived(): ResearchReport {
  return {
    ...baseReport(),
    id: "fixture-report-archived",
    job_id: "fixture-job-failed",
    title: "Fixture Report Archived",
    status: "archived",
    report_path:
      "/research/jobs/fixture-report-archived/fixture-report-archived.json",
    csv_path:
      "/research/jobs/fixture-report-archived/fixture-report-archived.csv",
  };
}

export function fixtureReportMissingFile(): ResearchReport {
  return {
    ...baseReport(),
    id: "fixture-report-missing-file",
    job_id: "fixture-job-blocked",
    title: "Fixture Report Missing File",
    status: "available",
    report_path: "/research/jobs/fixture-job-blocked/missing-report.json",
    csv_path: "/research/jobs/fixture-job-blocked/missing-report.csv",
  };
}

export function fixtureReportJsonPayload(): Record<string, unknown> {
  return {
    fixture: true,
    metrics: {
      net_pnl: 284.25,
      max_drawdown: -91.4,
      win_rate: 0.58,
      trade_count: 43,
    },
    params: {
      balance: 10000,
      EDGE_BPS: 2.5,
      TAKE_PROFIT_BPS: 18,
    },
    equity_curve: [
      { ts: FIXTURE_INTERVAL_START_MS, equity: 10000.0 },
      { ts: FIXTURE_INTERVAL_START_MS + 1_200_000, equity: 10056.1 },
      { ts: FIXTURE_INTERVAL_START_MS + 2_400_000, equity: 10142.9 },
      { ts: FIXTURE_INTERVAL_START_MS + 3_600_000, equity: 10210.7 },
      { ts: FIXTURE_INTERVAL_START_MS + 4_800_000, equity: 10284.25 },
    ],
    sweep_points: [
      { EDGE_BPS: 1.0, net_pnl: 122.4 },
      { EDGE_BPS: 2.5, net_pnl: 284.25 },
      { EDGE_BPS: 4.0, net_pnl: 198.7 },
    ],
  };
}

export function fixtureReportCsvPayload(): string {
  return [
    "metric,value",
    "net_pnl,284.25",
    "max_drawdown,-91.4",
    "win_rate,0.58",
    "trade_count,43",
    "",
  ].join("\n");
}
