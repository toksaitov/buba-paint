import { useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  FormField,
  SectionCard,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { JobCreateForm } from "../components/research/job-create-form";
import { initialValuesFromTemplate } from "../components/research/job-form-values";
import { RoleGate } from "../components/research/role-gate";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useResearchJobTemplates } from "../hooks/use-research-templates";
import { useAuthStore } from "../stores/auth-store";
import { createResearchJob } from "../lib/research-api";
import type {
  CreateJobRequest,
  JobType,
} from "../lib/research-types";

const ALLOWED_TYPES: JobType[] = ["export", "current_params", "sweep"];

function parseType(value: string | null): JobType | undefined {
  if (!value) return undefined;
  return ALLOWED_TYPES.includes(value as JobType)
    ? (value as JobType)
    : undefined;
}

export function ResearchJobNewPage() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;
  const returnToJobs = useResearchReturnTo("jobs", "/research/jobs");

  const artifactsQuery = useResearchArtifacts();
  const templatesQuery = useResearchJobTemplates();
  const [selectedTemplateId, setSelectedTemplateId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const templates = useMemo(
    () =>
      (templatesQuery.data?.templates ?? []).filter(
        (template) => template.status === "active",
      ),
    [templatesQuery.data],
  );
  const selectedTemplate = templates.find(
    (template) => template.id === selectedTemplateId,
  );
  const requestedArtifactId = params.get("artifact") ?? "";
  const artifactInitialValues = useMemo(() => {
    if (!requestedArtifactId) return undefined;
    const exists = (artifactsQuery.data?.artifacts ?? []).some(
      (artifact) => artifact.id === requestedArtifactId,
    );
    if (!exists) return undefined;
    return {
      backtest: { artifact_id: requestedArtifactId },
      sweep: { artifact_id: requestedArtifactId },
    };
  }, [requestedArtifactId, artifactsQuery.data]);

  const mutation = useMutation({
    mutationFn: (req: CreateJobRequest) => createResearchJob(req),
    onSuccess: (res) => {
      queryClient.invalidateQueries({ queryKey: ["research", "jobs"] });
      navigate(`/research/jobs/${encodeURIComponent(res.job.id)}`);
    },
    onError: (err: Error) => setError(err.message),
  });

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToJobs}
          className="text-[12px] text-muted hover:underline"
        >
          ← Jobs
        </Link>
        <span className="text-[14px] font-semibold">New job</span>
      </div>
      <RoleGate
        role={role}
        action="create"
        message="Only admins can create research jobs. Observers can browse details."
      >
        <SectionCard title="Job parameters">
          {artifactsQuery.isLoading ? (
            <Loading label="Loading artifacts" />
          ) : artifactsQuery.isError ? (
            <Banner tone="danger" title="Could not load artifacts">
              {(artifactsQuery.error as Error)?.message ?? "Unknown error"}
            </Banner>
          ) : (
            <div className="space-y-3">
              <FormField label="Template" hint="Optional shared defaults">
                {({ id }) => (
                  <select
                    id={id}
                    value={selectedTemplateId}
                    onChange={(event) =>
                      setSelectedTemplateId(event.currentTarget.value)
                    }
                    className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
                    disabled={templatesQuery.isLoading}
                  >
                    <option value="">No template</option>
                    {templates.map((template) => (
                      <option key={template.id} value={template.id}>
                        {template.name}
                      </option>
                    ))}
                  </select>
                )}
              </FormField>
              {templatesQuery.isError && (
                <Banner tone="warning" title="Could not load templates">
                  {(templatesQuery.error as Error)?.message ?? "Unknown error"}
                </Banner>
              )}
              <JobCreateForm
                key={
                  selectedTemplate?.id ??
                  (artifactInitialValues ? requestedArtifactId : "manual")
                }
                artifacts={artifactsQuery.data?.artifacts ?? []}
                initialType={
                  selectedTemplate?.job_type ??
                  parseType(params.get("type")) ??
                  "current_params"
                }
                initialValues={
                  selectedTemplate
                    ? initialValuesFromTemplate(selectedTemplate)
                    : artifactInitialValues
                }
                typeLocked={selectedTemplate != null}
                showPriority
                submitLabel={
                  selectedTemplate ? "Create from template" : "Create job"
                }
                pending={mutation.isPending}
                error={error}
                onSubmit={(req: CreateJobRequest) => {
                  setError(null);
                  mutation.mutate({
                    ...req,
                    template_id: selectedTemplate?.id,
                  });
                }}
                onCancel={() => navigate(returnToJobs)}
              />
            </div>
          )}
        </SectionCard>
      </RoleGate>
    </div>
  );
}
