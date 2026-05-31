import { del, get, getText, patch, post } from "./api";
import type {
  AppendEventRequest,
  ArchiveScratchResponse,
  ArtifactManifest,
  ArtifactTransfer,
  CloneJobRequest,
  CreateJobRequest,
  CreateMachineRequest,
  CreateTransferRequest,
  ImportArtifactRequest,
  ImportArtifactResponse,
  JobDetailResponse,
  MachineHealthResponse,
  MachineTelemetryResponse,
  RegenerateReportResponse,
  RegisterArtifactRequest,
  RegisterArtifactResponse,
  ResearchJobTemplate,
  ResearchQueueResponse,
  ResearchRetentionResponse,
  RetentionArchiveRequest,
  RetentionArchiveResponse,
  ResearchArtifact,
  ResearchJob,
  ResearchJobEvent,
  ResearchMachine,
  ResearchReport,
  RetryTransferRequest,
  TransferProgressRequest,
  UpdateArtifactRequest,
  UpdateJobRequest,
  UpsertJobTemplateRequest,
  UpdateMachineRequest,
  UpdateReportRequest,
  VerifyArtifactResponse,
  VerifyTransferResponse,
} from "./research-types";

export async function listResearchMachines(): Promise<{
  machines: ResearchMachine[];
}> {
  return get("/api/research/machines");
}

export async function createResearchMachine(
  req: CreateMachineRequest,
): Promise<{ machine: ResearchMachine }> {
  return post("/api/research/machines", req);
}

export async function getResearchMachine(
  id: string,
): Promise<{ machine: ResearchMachine }> {
  return get(`/api/research/machines/${encodeURIComponent(id)}`);
}

export async function updateResearchMachine(
  id: string,
  req: UpdateMachineRequest,
): Promise<{ machine: ResearchMachine }> {
  return patch(`/api/research/machines/${encodeURIComponent(id)}`, req);
}

export async function disableResearchMachine(
  id: string,
): Promise<{ machine: ResearchMachine }> {
  return post(`/api/research/machines/${encodeURIComponent(id)}/disable`);
}

export async function enableResearchMachine(
  id: string,
): Promise<{ machine: ResearchMachine }> {
  return post(`/api/research/machines/${encodeURIComponent(id)}/enable`);
}

export async function deleteResearchMachine(
  id: string,
): Promise<{ machine: ResearchMachine }> {
  return del(`/api/research/machines/${encodeURIComponent(id)}`);
}

export async function getResearchMachineHealth(
  id: string,
): Promise<MachineHealthResponse> {
  return get(`/api/research/machines/${encodeURIComponent(id)}/health`);
}

export async function getResearchMachineTelemetry(
  id: string,
): Promise<MachineTelemetryResponse> {
  return get(`/api/research/machines/${encodeURIComponent(id)}/telemetry`);
}

export async function listResearchArtifacts(): Promise<{
  artifacts: ResearchArtifact[];
}> {
  return get("/api/research/artifacts");
}

export async function importResearchArtifact(
  req: ImportArtifactRequest,
): Promise<ImportArtifactResponse> {
  return post("/api/research/artifacts/import", req);
}

export async function registerResearchArtifact(
  req: RegisterArtifactRequest,
): Promise<RegisterArtifactResponse> {
  return post("/api/research/artifacts/register", req);
}

export async function getResearchArtifact(
  id: string,
): Promise<ResearchArtifact> {
  return get(`/api/research/artifacts/${encodeURIComponent(id)}`);
}

export async function updateResearchArtifact(
  id: string,
  req: UpdateArtifactRequest,
): Promise<ResearchArtifact> {
  return patch(`/api/research/artifacts/${encodeURIComponent(id)}`, req);
}

export async function verifyResearchArtifact(
  id: string,
): Promise<VerifyArtifactResponse> {
  return post(`/api/research/artifacts/${encodeURIComponent(id)}/verify`);
}

export async function archiveResearchArtifact(
  id: string,
): Promise<ResearchArtifact> {
  return post(`/api/research/artifacts/${encodeURIComponent(id)}/archive`);
}

export async function restoreResearchArtifact(
  id: string,
): Promise<ResearchArtifact> {
  return post(`/api/research/artifacts/${encodeURIComponent(id)}/restore`);
}

export async function deleteResearchArtifact(
  id: string,
  deleteFiles: boolean,
): Promise<ResearchArtifact> {
  return del(
    `/api/research/artifacts/${encodeURIComponent(id)}`,
    deleteFiles ? { delete_files: "true" } : undefined,
  );
}

export async function getResearchArtifactManifest(
  id: string,
): Promise<ArtifactManifest> {
  return get(`/api/research/artifacts/${encodeURIComponent(id)}/manifest`);
}

export async function getResearchArtifactChecksums(
  id: string,
): Promise<string> {
  return getText(
    `/api/research/artifacts/${encodeURIComponent(id)}/checksums`,
  );
}

export async function listArtifactTransfers(): Promise<{
  transfers: ArtifactTransfer[];
}> {
  return get("/api/research/transfers");
}

export async function createArtifactTransfer(
  req: CreateTransferRequest,
): Promise<ArtifactTransfer> {
  return post("/api/research/transfers", req);
}

export async function getArtifactTransfer(
  id: string,
): Promise<ArtifactTransfer> {
  return get(`/api/research/transfers/${encodeURIComponent(id)}`);
}

export async function updateArtifactTransferProgress(
  id: string,
  req: TransferProgressRequest,
): Promise<ArtifactTransfer> {
  return post(
    `/api/research/transfers/${encodeURIComponent(id)}/progress`,
    req,
  );
}

export async function cancelArtifactTransfer(
  id: string,
): Promise<ArtifactTransfer> {
  return post(`/api/research/transfers/${encodeURIComponent(id)}/cancel`);
}

export async function pauseArtifactTransfer(
  id: string,
): Promise<ArtifactTransfer> {
  return post(`/api/research/transfers/${encodeURIComponent(id)}/pause`);
}

export async function resumeArtifactTransfer(
  id: string,
): Promise<ArtifactTransfer> {
  return post(`/api/research/transfers/${encodeURIComponent(id)}/resume`);
}

export async function retryArtifactTransfer(
  id: string,
  req: RetryTransferRequest = {},
): Promise<ArtifactTransfer> {
  return post(
    `/api/research/transfers/${encodeURIComponent(id)}/retry`,
    req,
  );
}

export async function verifyArtifactTransfer(
  id: string,
): Promise<VerifyTransferResponse> {
  return post(`/api/research/transfers/${encodeURIComponent(id)}/verify`);
}

export async function deleteArtifactTransfer(
  id: string,
): Promise<ArtifactTransfer> {
  return del(`/api/research/transfers/${encodeURIComponent(id)}`);
}

export async function listResearchJobs(): Promise<{ jobs: ResearchJob[] }> {
  return get("/api/research/jobs");
}

export async function createResearchJob(
  req: CreateJobRequest,
): Promise<JobDetailResponse> {
  return post("/api/research/jobs", req);
}

export async function listResearchJobTemplates(): Promise<{
  templates: ResearchJobTemplate[];
}> {
  return get("/api/research/job-templates");
}

export async function createResearchJobTemplate(
  req: UpsertJobTemplateRequest,
): Promise<{ template: ResearchJobTemplate }> {
  return post("/api/research/job-templates", req);
}

export async function getResearchJobTemplate(
  id: string,
): Promise<{ template: ResearchJobTemplate }> {
  return get(`/api/research/job-templates/${encodeURIComponent(id)}`);
}

export async function updateResearchJobTemplate(
  id: string,
  req: UpsertJobTemplateRequest,
): Promise<{ template: ResearchJobTemplate }> {
  return patch(`/api/research/job-templates/${encodeURIComponent(id)}`, req);
}

export async function archiveResearchJobTemplate(
  id: string,
): Promise<{ template: ResearchJobTemplate }> {
  return post(`/api/research/job-templates/${encodeURIComponent(id)}/archive`);
}

export async function restoreResearchJobTemplate(
  id: string,
): Promise<{ template: ResearchJobTemplate }> {
  return post(`/api/research/job-templates/${encodeURIComponent(id)}/restore`);
}

export async function deleteResearchJobTemplate(
  id: string,
): Promise<{ template: ResearchJobTemplate }> {
  return del(`/api/research/job-templates/${encodeURIComponent(id)}`);
}

export async function getResearchQueue(): Promise<ResearchQueueResponse> {
  return get("/api/research/queue");
}

export async function getResearchRetention(): Promise<ResearchRetentionResponse> {
  return get("/api/research/retention");
}

export async function archiveResearchRetention(
  req: RetentionArchiveRequest,
): Promise<RetentionArchiveResponse> {
  return post("/api/research/retention/archive", req);
}

export async function getResearchJob(id: string): Promise<JobDetailResponse> {
  return get(`/api/research/jobs/${encodeURIComponent(id)}`);
}

export async function updateResearchJob(
  id: string,
  req: UpdateJobRequest,
): Promise<JobDetailResponse> {
  return patch(`/api/research/jobs/${encodeURIComponent(id)}`, req);
}

export async function cancelResearchJob(
  id: string,
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/cancel`);
}

export async function pauseResearchJob(
  id: string,
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/pause`);
}

export async function resumeResearchJob(
  id: string,
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/resume`);
}

export async function continueResearchJob(
  id: string,
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/continue`);
}

export async function retryResearchJob(
  id: string,
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/retry`);
}

export async function cloneResearchJob(
  id: string,
  req: CloneJobRequest = {},
): Promise<JobDetailResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/clone`, req);
}

export async function deleteResearchJob(id: string): Promise<ResearchJob> {
  return del(`/api/research/jobs/${encodeURIComponent(id)}`);
}

export async function regenerateResearchJobReport(
  id: string,
): Promise<RegenerateReportResponse> {
  return post(
    `/api/research/jobs/${encodeURIComponent(id)}/report/regenerate`,
  );
}

export async function archiveResearchJobScratch(
  id: string,
): Promise<ArchiveScratchResponse> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/archive-scratch`);
}

export async function listResearchJobEvents(
  id: string,
): Promise<{ events: ResearchJobEvent[] }> {
  return get(`/api/research/jobs/${encodeURIComponent(id)}/events`);
}

export async function appendResearchJobEvent(
  id: string,
  req: AppendEventRequest,
): Promise<ResearchJobEvent> {
  return post(`/api/research/jobs/${encodeURIComponent(id)}/events`, req);
}

export async function retryResearchStep(
  jobId: string,
  stepId: string,
): Promise<JobDetailResponse> {
  return post(
    `/api/research/jobs/${encodeURIComponent(jobId)}/steps/${encodeURIComponent(
      stepId,
    )}/retry`,
  );
}

export async function cancelResearchStep(
  jobId: string,
  stepId: string,
): Promise<JobDetailResponse> {
  return post(
    `/api/research/jobs/${encodeURIComponent(jobId)}/steps/${encodeURIComponent(
      stepId,
    )}/cancel`,
  );
}

export async function clearResearchStepLease(
  jobId: string,
  stepId: string,
): Promise<JobDetailResponse> {
  return post(
    `/api/research/jobs/${encodeURIComponent(jobId)}/steps/${encodeURIComponent(
      stepId,
    )}/clear-lease`,
  );
}

export async function resolveResearchStepBlocker(
  jobId: string,
  stepId: string,
): Promise<JobDetailResponse> {
  return post(
    `/api/research/jobs/${encodeURIComponent(jobId)}/steps/${encodeURIComponent(
      stepId,
    )}/resolve-blocker`,
  );
}

export async function listResearchReports(): Promise<{
  reports: ResearchReport[];
}> {
  return get("/api/research/reports");
}

export async function getResearchReport(
  id: string,
): Promise<ResearchReport> {
  return get(`/api/research/reports/${encodeURIComponent(id)}`);
}

export async function updateResearchReport(
  id: string,
  req: UpdateReportRequest,
): Promise<ResearchReport> {
  return patch(`/api/research/reports/${encodeURIComponent(id)}`, req);
}

export async function archiveResearchReport(
  id: string,
): Promise<ResearchReport> {
  return post(`/api/research/reports/${encodeURIComponent(id)}/archive`);
}

export async function restoreResearchReport(
  id: string,
): Promise<ResearchReport> {
  return post(`/api/research/reports/${encodeURIComponent(id)}/restore`);
}

export async function deleteResearchReport(
  id: string,
  deleteFiles: boolean,
): Promise<ResearchReport> {
  return del(
    `/api/research/reports/${encodeURIComponent(id)}`,
    deleteFiles ? { delete_files: "true" } : undefined,
  );
}

export async function getResearchReportJson(
  id: string,
): Promise<unknown> {
  return get(`/api/research/reports/${encodeURIComponent(id)}/json`);
}

export async function getResearchReportCsv(id: string): Promise<string> {
  return getText(`/api/research/reports/${encodeURIComponent(id)}/csv`);
}

export function downloadResearchReportCsvFromText(
  csv: string,
  filename: string,
): void {
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}
