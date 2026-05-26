import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Plus } from "lucide-react";
import {
  Banner,
  Button,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchJobs } from "../hooks/use-research-jobs";
import { useAuthStore } from "../stores/auth-store";
import { jobTone, jobTypeLabel } from "../lib/research-permissions";
import type { JobStatus, JobType } from "../lib/research-types";
import { humanize } from "../lib/utils";

const ALL_STATUSES: JobStatus[] = [
  "queued",
  "running",
  "retryable",
  "paused",
  "blocked",
  "failed",
  "cancelled",
  "completed",
];

const TYPE_OPTIONS: ({ value: "all" | JobType; label: string })[] = [
  { value: "all", label: "All types" },
  { value: "export", label: "Export" },
  { value: "current_params", label: "Backtest" },
  { value: "sweep", label: "Sweep" },
];

export function ResearchJobsPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const navigate = useNavigate();
  const jobsQuery = useResearchJobs();
  const [active, setActive] = useState<string[]>([...ALL_STATUSES]);
  const [typeFilter, setTypeFilter] = useState<"all" | JobType>("all");

  const jobsData = jobsQuery.data?.jobs;
  const filtered = useMemo(
    () =>
      [...(jobsData ?? [])]
        .filter((j) => active.includes(j.status))
        .filter((j) => typeFilter === "all" || j.job_type === typeFilter)
        .sort((a, b) => b.updated_at - a.updated_at),
    [jobsData, active, typeFilter],
  );

  if (jobsQuery.isLoading) {
    return <Loading label="Loading jobs" />;
  }
  if (jobsQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load jobs">
        {(jobsQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard
        title="All jobs"
        toolbar={
          <div className="flex items-center gap-2">
            <select
              aria-label="Job type filter"
              value={typeFilter}
              onChange={(e) =>
                setTypeFilter(e.currentTarget.value as "all" | JobType)
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {TYPE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
            <Button
              size="sm"
              tone="accent"
              iconLeft={<Plus size={14} />}
              onClick={() => navigate("/research/jobs/new")}
              disabled={!isAdmin}
              title={isAdmin ? undefined : "Admin role required."}
            >
              New job
            </Button>
          </div>
        }
      >
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => jobTone(s as JobStatus)}
          ariaLabel="Job status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No jobs yet — create an export, backtest, or sweep job." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">ID</th>
                  <th className="px-2 py-1.5 font-semibold">Type</th>
                  <th className="px-2 py-1.5 font-semibold">Artifact</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">Priority</th>
                  <th className="px-2 py-1.5 font-semibold">Requested by</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((job) => (
                  <tr
                    key={job.id}
                    className="border-b border-border last:border-b-0 hover:bg-surface"
                  >
                    <td className="px-2 py-1.5">
                      <Link
                        to={`/research/jobs/${encodeURIComponent(job.id)}`}
                        className="font-mono text-[11px] hover:underline"
                      >
                        {job.id}
                      </Link>
                    </td>
                    <td className="px-2 py-1.5">
                      <StatusChip
                        label={jobTypeLabel(job.job_type)}
                        tone="neutral"
                        compact
                      />
                    </td>
                    <td className="px-2 py-1.5">
                      {job.artifact_id ? (
                        <Link
                          to={`/research/artifacts/${encodeURIComponent(job.artifact_id)}`}
                          className="font-mono text-[11px] hover:underline"
                        >
                          {job.artifact_id}
                        </Link>
                      ) : (
                        <span className="text-muted">—</span>
                      )}
                    </td>
                    <td className="px-2 py-1.5">
                      <StatusChip
                        label={humanize(job.status)}
                        tone={jobTone(job.status)}
                        compact
                      />
                    </td>
                    <td className="px-2 py-1.5 tabular-nums">{job.priority}</td>
                    <td className="px-2 py-1.5 text-muted">
                      {job.requested_by}
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      <RelativeTime epochMs={job.updated_at} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
    </div>
  );
}
