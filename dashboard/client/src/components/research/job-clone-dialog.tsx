import { useMemo } from "react";
import {
  Banner,
  Button,
} from "../ui/dashboard-primitives";
import { Dialog } from "../ui/dialog";
import { JobCreateForm } from "./job-create-form";
import { initialValuesFromJob } from "./job-form-values";
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
  const hasAdditionalParams =
    (initialValues.additionalParamsJson?.trim().length ?? 0) > 0;
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
            {hasAdditionalParams
              ? "Clone keeps the original job type. Known fields are editable below; unknown source params remain in Additional params JSON."
              : "Clone keeps the original job type. Known fields are editable below."}
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
            showAdditionalParams={hasAdditionalParams}
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
