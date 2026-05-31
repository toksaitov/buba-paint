import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  Button,
  FormField,
  Input,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { EventStream } from "../components/research/event-stream";
import { JobCloneDialog } from "../components/research/job-clone-dialog";
import { parseRecord } from "../components/research/job-form-values";
import { JobRecoveryDiagnosis } from "../components/research/job-recovery-diagnosis";
import { JsonViewer } from "../components/research/json-viewer";
import { StepTimeline } from "../components/research/step-timeline";
import { Dialog } from "../components/ui/dialog";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchJob } from "../hooks/use-research-jobs";
import { useResearchReports } from "../hooks/use-research-reports";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useAuthStore } from "../stores/auth-store";
import {
  appendResearchJobEvent,
  archiveResearchJobScratch,
  cancelResearchJob,
  cancelResearchStep,
  clearResearchStepLease,
  cloneResearchJob,
  continueResearchJob,
  createResearchJobTemplate,
  deleteResearchJob,
  pauseResearchJob,
  regenerateResearchJobReport,
  resolveResearchStepBlocker,
  resumeResearchJob,
  retryResearchJob,
  retryResearchStep,
} from "../lib/research-api";
import {
  ACTION_LABELS,
  getActionGateState,
  getAllowedActions,
  isJobTerminal,
  jobTone,
  jobTypeLabel,
  permissionHint,
} from "../lib/research-permissions";
import type {
  AppendEventRequest,
  ArchiveScratchSummary,
  CloneJobRequest,
  JobDetailResponse,
  ResearchAction,
  ResearchJob,
} from "../lib/research-types";
import { humanize } from "../lib/utils";

export function ResearchJobDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;
  const returnToJobs = useResearchReturnTo("jobs", "/research/jobs");

  const jobQuery = useResearchJob(id);
  const reportsQuery = useResearchReports();
  const artifactsQuery = useResearchArtifacts();

  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<ResearchAction | null>(
    null,
  );
  const [pendingStepAction, setPendingStepAction] = useState<{
    stepId: string;
    action: ResearchAction;
  } | null>(null);
  const [appendError, setAppendError] = useState<string | null>(null);
  const [isAppending, setIsAppending] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [confirmArchiveScratchOpen, setConfirmArchiveScratchOpen] =
    useState(false);
  const [cloneDialogOpen, setCloneDialogOpen] = useState(false);
  const [saveTemplateOpen, setSaveTemplateOpen] = useState(false);
  const [archiveSummary, setArchiveSummary] =
    useState<ArchiveScratchSummary | null>(null);

  const invalidateJob = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "jobs"] });
    queryClient.invalidateQueries({ queryKey: ["research", "job", id] });
  };

  const runJobMutation = (
    action: ResearchAction,
    fn: () => Promise<JobDetailResponse>,
  ) => {
    setPendingAction(action);
    setActionError(null);
    fn()
      .then((res) => {
        setPendingAction(null);
        queryClient.setQueryData(["research", "job", id], res);
        invalidateJob();
      })
      .catch((err: Error) => {
        setPendingAction(null);
        setActionError(err.message);
      });
  };

  const cloneMutation = useMutation({
    mutationFn: (req: CloneJobRequest) => cloneResearchJob(id, req),
    onMutate: () => {
      setPendingAction("clone");
      setActionError(null);
    },
    onSuccess: (res) => {
      setPendingAction(null);
      setCloneDialogOpen(false);
      queryClient.invalidateQueries({ queryKey: ["research", "jobs"] });
      navigate(`/research/jobs/${encodeURIComponent(res.job.id)}`);
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const regenerateMutation = useMutation({
    mutationFn: () => regenerateResearchJobReport(id),
    onMutate: () => {
      setPendingAction("regenerate_report");
      setActionError(null);
    },
    onSuccess: () => {
      setPendingAction(null);
      queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
      queryClient.invalidateQueries({ queryKey: ["research", "job", id] });
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const archiveScratchMutation = useMutation({
    mutationFn: () => archiveResearchJobScratch(id),
    onMutate: () => {
      setPendingAction("archive_scratch");
      setActionError(null);
      setArchiveSummary(null);
    },
    onSuccess: (res) => {
      setPendingAction(null);
      setConfirmArchiveScratchOpen(false);
      setArchiveSummary(res.archive);
      queryClient.setQueryData(["research", "job", id], {
        job: res.job,
        steps: res.steps,
        events: res.events,
      });
      queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
      invalidateJob();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const saveTemplateMutation = useMutation({
    mutationFn: (req: { name: string; description?: string }) => {
      const job = jobQuery.data?.job;
      if (!job || (job.job_type !== "current_params" && job.job_type !== "sweep")) {
        throw new Error("Only backtest and sweep jobs can be saved as templates.");
      }
      const jobType = job.job_type;
      return createResearchJobTemplate({
        name: req.name,
        description: req.description,
        job_type: jobType,
        artifact_id: job.artifact_id ?? undefined,
        priority: job.priority,
        params: parseRecord(job.params_json),
      });
    },
    onSuccess: () => {
      setSaveTemplateOpen(false);
      queryClient.invalidateQueries({
        queryKey: ["research", "job-templates"],
      });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteResearchJob(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "jobs"] });
      navigate(returnToJobs);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const handleJobAction = (action: ResearchAction) => {
    switch (action) {
      case "cancel":
        return runJobMutation("cancel", () => cancelResearchJob(id));
      case "pause":
        return runJobMutation("pause", () => pauseResearchJob(id));
      case "resume":
        return runJobMutation("resume", () => resumeResearchJob(id));
      case "continue":
        return runJobMutation("continue", () => continueResearchJob(id));
      case "retry":
        return runJobMutation("retry", () => retryResearchJob(id));
      case "clone":
        cloneMutation.reset();
        setActionError(null);
        return setCloneDialogOpen(true);
      case "regenerate_report":
        return regenerateMutation.mutate();
      case "archive_scratch":
        return setConfirmArchiveScratchOpen(true);
      case "delete":
        return setConfirmDeleteOpen(true);
      default:
        return undefined;
    }
  };

  const handleStepAction = (stepId: string, action: ResearchAction) => {
    setPendingStepAction({ stepId, action });
    setActionError(null);
    const fn = (() => {
      switch (action) {
        case "retry":
          return () => retryResearchStep(id, stepId);
        case "cancel":
          return () => cancelResearchStep(id, stepId);
        case "clear_lease":
          return () => clearResearchStepLease(id, stepId);
        case "resolve_blocker":
          return () => resolveResearchStepBlocker(id, stepId);
        default:
          return null;
      }
    })();
    if (!fn) {
      setPendingStepAction(null);
      return;
    }
    fn()
      .then((res) => {
        setPendingStepAction(null);
        queryClient.setQueryData(["research", "job", id], res);
        invalidateJob();
      })
      .catch((err: Error) => {
        setPendingStepAction(null);
        setActionError(err.message);
      });
  };

  const handleAppendEvent = (req: AppendEventRequest) => {
    setIsAppending(true);
    setAppendError(null);
    return appendResearchJobEvent(id, req)
      .then(() => {
        setIsAppending(false);
        queryClient.invalidateQueries({ queryKey: ["research", "job", id] });
      })
      .catch((err: Error) => {
        setIsAppending(false);
        setAppendError(err.message);
        throw err;
      });
  };

  const linkedReport = useMemo(() => {
    const reports = reportsQuery.data?.reports ?? [];
    return reports.find((r) => r.job_id === id);
  }, [reportsQuery.data, id]);

  if (jobQuery.isLoading) {
    return <Loading label="Loading job" />;
  }
  if (jobQuery.isError || !jobQuery.data) {
    return (
      <Banner tone="danger" title="Could not load job">
        {(jobQuery.error as Error)?.message ?? "Job not found."}
      </Banner>
    );
  }

  const { job, steps, events } = jobQuery.data;
  const hasReports = linkedReport != null;
  const allowed = getAllowedActions("job", job.status, {
    has_reports: hasReports,
  });
  const terminal = isJobTerminal(job.status);
  const canSaveTemplate =
    role === "admin" &&
    (job.job_type === "current_params" || job.job_type === "sweep");

  const blockedStep = steps.find((s) => s.status === "blocked");
  const failedStep =
    job.status === "failed" ? steps.find((s) => s.status === "failed") : null;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToJobs}
          className="text-[12px] text-muted hover:underline"
        >
          ← Jobs
        </Link>
        <span className="font-mono text-[12px]">{job.id}</span>
        <StatusChip label={jobTypeLabel(job.job_type)} tone="neutral" />
        <StatusChip label={humanize(job.status)} tone={jobTone(job.status)} />
        <span className="text-[12px] text-muted">
          updated <RelativeTime epochMs={job.updated_at} />
        </span>
      </div>

      {actionError && (
        <Banner tone="danger" title="Action failed">
          {actionError}
        </Banner>
      )}

      {blockedStep && (
        <Banner tone="info" title="Job is blocked">
          Step {blockedStep.step_index + 1} ({humanize(blockedStep.name)}) is
          blocked. Mark the blocker resolved or retry to continue.
        </Banner>
      )}
      {failedStep && (
        <Banner tone="danger" title="Job failed">
          <pre className="whitespace-pre-wrap font-mono text-[11px]">
            {failedStep.error ?? "No error message recorded."}
          </pre>
        </Banner>
      )}
      {(job.status === "running" || job.status === "queued") &&
        pendingAction === "cancel" && (
        <Banner tone="info" title="Cancellation in flight">
            The durable state has been updated. A running worker process will
            terminate its active command when it observes the cancellation.
        </Banner>
      )}
      {archiveSummary && (
        <Banner tone="success" title="Scratch DB archive complete">
          Deleted {archiveSummary.deleted_paths.length} scratch file
          {archiveSummary.deleted_paths.length === 1 ? "" : "s"}; skipped{" "}
          {archiveSummary.skipped_paths.length} already-absent file
          {archiveSummary.skipped_paths.length === 1 ? "" : "s"}.
        </Banner>
      )}

      <JobRecoveryDiagnosis job={job} steps={steps} events={events} />

      <SectionCard title="Actions">
        <div className="flex flex-wrap items-center gap-2">
          {allowed.map((action) => {
            const gate = getActionGateState(role, action);
            const canPreviewClone =
              action === "clone" && gate !== "enabled";
            const disabled = gate !== "enabled" && !canPreviewClone;
            const isPending = pendingAction === action;
            const tone =
              action === "cancel" || action === "delete" ? "danger" : "neutral";
            if (action === "append_event") return null;
            return (
              <Button
                key={action}
                size="sm"
                tone={tone}
                disabled={disabled || isPending}
                state={isPending ? "pending" : "idle"}
                title={
                  disabled
                    ? permissionHint(action)
                    : canPreviewClone
                      ? "Open clone preview. Admin role required to submit."
                      : undefined
                }
                onClick={() => handleJobAction(action)}
              >
                {ACTION_LABELS[action]}
              </Button>
            );
          })}
          <Button
            size="sm"
            disabled={!canSaveTemplate}
            title={
              canSaveTemplate
                ? undefined
                : "Admin role and a backtest or sweep job are required."
            }
            onClick={() => {
              saveTemplateMutation.reset();
              setSaveTemplateOpen(true);
            }}
          >
            Save as template
          </Button>
          {!allowed.includes("delete") && hasReports && terminal && (
            <span className="text-[11px] text-muted">
              Delete unavailable: a report references this job. Delete the
              report first.
            </span>
          )}
        </div>
      </SectionCard>

      <SectionCard title="Details">
        <div className="space-y-3">
          <KeyValueList
            columns={2}
            items={[
              { label: "Type", value: jobTypeLabel(job.job_type) },
              {
                label: "Artifact",
                value: job.artifact_id ? (
                  <Link
                    to={`/research/artifacts/${encodeURIComponent(job.artifact_id)}`}
                    className="font-mono text-[11px] hover:underline"
                  >
                    {job.artifact_id}
                  </Link>
                ) : (
                  "—"
                ),
              },
              { label: "Requested by", value: job.requested_by },
              { label: "Priority", value: job.priority },
              {
                label: "Created",
                value: <RelativeTime epochMs={job.created_at} />,
              },
              {
                label: "Updated",
                value: <RelativeTime epochMs={job.updated_at} />,
              },
              {
                label: "Cancelled",
                value: job.cancelled_at ? (
                  <RelativeTime epochMs={job.cancelled_at} />
                ) : (
                  "—"
                ),
              },
              {
                label: "Completed",
                value: job.completed_at ? (
                  <RelativeTime epochMs={job.completed_at} />
                ) : (
                  "—"
                ),
              },
            ]}
          />
          <JsonViewer
            value={job.params_json}
            label="Params"
            emptyLabel="No params recorded"
            maxHeight={240}
          />
        </div>
      </SectionCard>

      <SectionCard title={`Steps (${steps.length})`}>
        <StepTimeline
          steps={steps}
          role={role}
          pendingStepAction={pendingStepAction}
          onAction={handleStepAction}
        />
      </SectionCard>

      <SectionCard title={`Events (${events.length})`}>
        <EventStream
          events={events}
          role={role}
          onAppend={handleAppendEvent}
          isAppending={isAppending}
          appendError={appendError}
        />
      </SectionCard>

      <SectionCard title="Report">
        {!linkedReport ? (
          <p className="text-[12px] text-muted">
            No report yet. For completed backtest/sweep jobs, click{" "}
            <span className="font-semibold">Regenerate report</span> to write
            one.
          </p>
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <Link
              to={`/research/reports/${encodeURIComponent(linkedReport.id)}`}
              className="text-[12px] font-semibold hover:underline"
            >
              {linkedReport.title}
            </Link>
            <span className="font-mono text-[11px] text-muted">
              {linkedReport.id}
            </span>
            <StatusChip label={humanize(linkedReport.status)} compact />
          </div>
        )}
      </SectionCard>

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="Delete job"
        description="Type the job ID to confirm. Deletes the job, its steps, and its events. Linked reports remain but should be reviewed."
        phrase={job.id}
        confirmLabel="Delete job"
        destructive
        pending={deleteMutation.isPending}
        errorMessage={
          deleteMutation.isError
            ? (deleteMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteMutation.mutate()}
        onClose={() => setConfirmDeleteOpen(false)}
      />
      <ConfirmDialog
        open={confirmArchiveScratchOpen}
        title="Archive scratch DBs"
        description="Type the job ID to confirm. Deletes only prepared/backtest scratch SQLite files and WAL/SHM sidecars under this job root. Report JSON, CSV, metadata, artifacts, and manifests remain."
        phrase={job.id}
        confirmLabel="Archive scratch DBs"
        pending={archiveScratchMutation.isPending}
        errorMessage={
          archiveScratchMutation.isError
            ? (archiveScratchMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => archiveScratchMutation.mutate()}
        onClose={() => setConfirmArchiveScratchOpen(false)}
      />
      <JobCloneDialog
        open={cloneDialogOpen}
        job={job}
        artifacts={artifactsQuery.data?.artifacts ?? []}
        loadingArtifacts={artifactsQuery.isLoading}
        artifactError={
          artifactsQuery.isError
            ? ((artifactsQuery.error as Error)?.message ?? "Unknown error")
            : null
        }
        role={role}
        pending={cloneMutation.isPending}
        error={
          cloneMutation.isError
            ? ((cloneMutation.error as Error)?.message ?? "Clone failed")
            : null
        }
        onSubmit={(req) => cloneMutation.mutate(req)}
        onClose={() => {
          setCloneDialogOpen(false);
          cloneMutation.reset();
        }}
      />
      {saveTemplateOpen && (
        <SaveTemplateDialog
          open={saveTemplateOpen}
          job={job}
          pending={saveTemplateMutation.isPending}
          error={
            saveTemplateMutation.isError
              ? ((saveTemplateMutation.error as Error)?.message ??
                "Could not save template")
              : null
          }
          onSubmit={(req) => saveTemplateMutation.mutate(req)}
          onClose={() => {
            setSaveTemplateOpen(false);
            saveTemplateMutation.reset();
          }}
        />
      )}
    </div>
  );
}

interface SaveTemplateDialogProps {
  open: boolean;
  job: ResearchJob;
  pending: boolean;
  error: string | null;
  onSubmit: (req: { name: string; description?: string }) => void;
  onClose: () => void;
}

function SaveTemplateDialog({
  open,
  job,
  pending,
  error,
  onSubmit,
  onClose,
}: SaveTemplateDialogProps) {
  const [name, setName] = useState(`Template from ${job.id.slice(0, 8)}`);
  const [description, setDescription] = useState("");
  const canSubmit = name.trim().length > 0 && !pending;

  return (
    <Dialog
      open={open}
      onClose={pending ? () => undefined : onClose}
      title="Save job as template"
      description="Stores shared defaults for future backtest or sweep jobs."
    >
      <div className="space-y-3">
        {error && (
          <Banner tone="danger" title="Template save failed">
            {error}
          </Banner>
        )}
        <FormField label="Name" required>
          {({ id }) => (
            <Input
              id={id}
              value={name}
              onChange={(event) => setName(event.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Description" hint="Optional">
          {({ id }) => (
            <Textarea
              id={id}
              value={description}
              onChange={(event) => setDescription(event.currentTarget.value)}
              minRows={3}
            />
          )}
        </FormField>
        <div className="text-[12px] text-muted">
          Job {job.id} · {jobTypeLabel(job.job_type)} · priority {job.priority}
        </div>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose} disabled={pending}>
            Cancel
          </Button>
          <Button
            tone="accent"
            state={pending ? "pending" : "idle"}
            disabled={!canSubmit}
            onClick={() =>
              onSubmit({
                name: name.trim(),
                description: description.trim() || undefined,
              })
            }
          >
            Save template
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
