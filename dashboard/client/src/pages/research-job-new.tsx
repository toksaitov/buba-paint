import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  SectionCard,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { JobCreateForm } from "../components/research/job-create-form";
import { RoleGate } from "../components/research/role-gate";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
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

  const artifactsQuery = useResearchArtifacts();
  const [error, setError] = useState<string | null>(null);

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
          to="/research/jobs"
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
            <JobCreateForm
              artifacts={artifactsQuery.data?.artifacts ?? []}
              initialType={parseType(params.get("type")) ?? "current_params"}
              pending={mutation.isPending}
              error={error}
              onSubmit={(req) => {
                setError(null);
                mutation.mutate(req);
              }}
              onCancel={() => navigate("/research/jobs")}
            />
          )}
        </SectionCard>
      </RoleGate>
    </div>
  );
}
