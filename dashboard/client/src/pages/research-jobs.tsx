import { useMemo } from "react";
import {
  Link,
  useLocation,
  useNavigate,
  useSearchParams,
} from "react-router-dom";
import { Plus } from "lucide-react";
import {
  Banner,
  Button,
  Input,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchJobs } from "../hooks/use-research-jobs";
import { useResearchReports } from "../hooks/use-research-reports";
import { useRememberResearchListReturn } from "../hooks/use-research-return-to";
import { useResearchQueue } from "../hooks/use-research-templates";
import { useAuthStore } from "../stores/auth-store";
import {
  isJobTerminal,
  jobTone,
  jobTypeLabel,
} from "../lib/research-permissions";
import type { JobStatus, JobType } from "../lib/research-types";
import {
  readEnumListParam,
  readEnumParam,
  readTextParam,
  updateQueryListParam,
  updateQueryParam,
} from "../lib/research-list-url-state";
import { humanize, truncateMiddle } from "../lib/utils";

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

const PRESET_OPTIONS = [
  { value: "all", label: "All jobs" },
  { value: "active", label: "Active" },
  { value: "attention", label: "Attention" },
  { value: "completed", label: "Completed" },
  { value: "cancelled", label: "Cancelled" },
  { value: "stale_lease", label: "Stale lease" },
  { value: "delete_eligible", label: "Delete eligible" },
] as const;

type JobPreset = (typeof PRESET_OPTIONS)[number]["value"];

const SORT_OPTIONS = [
  { value: "updated_desc", label: "Updated" },
  { value: "priority_desc", label: "Priority" },
  { value: "created_desc", label: "Created" },
] as const;

type JobSort = (typeof SORT_OPTIONS)[number]["value"];
const DEFAULT_SORT: JobSort = "updated_desc";

export function ResearchJobsPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  useRememberResearchListReturn("jobs", "/research/jobs");
  const returnToJobs = `${location.pathname}${location.search}`;
  const jobsQuery = useResearchJobs();
  const reportsQuery = useResearchReports();
  const queueQuery = useResearchQueue();
  const active = readEnumListParam(
    searchParams,
    "status",
    ALL_STATUSES,
    ALL_STATUSES,
  );
  const typeFilter = readEnumParam(
    searchParams,
    "type",
    TYPE_OPTIONS.map((option) => option.value),
    "all",
  );
  const preset = readEnumParam(
    searchParams,
    "preset",
    PRESET_OPTIONS.map((option) => option.value),
    "all",
  );
  const search = readTextParam(searchParams, "q");
  const sortKey = readEnumParam(
    searchParams,
    "sort",
    SORT_OPTIONS.map((option) => option.value),
    DEFAULT_SORT,
  );

  const jobsData = jobsQuery.data?.jobs;
  const reportJobIds = useMemo(
    () => new Set((reportsQuery.data?.reports ?? []).map((report) => report.job_id)),
    [reportsQuery.data],
  );
  const staleJobIds = useMemo(
    () =>
      new Set(
        queueQuery.data?.jobs.stale_leases.map((item) => item.job.id) ?? [],
      ),
    [queueQuery.data],
  );
  const filtered = useMemo(
    () => {
      const needle = search.trim().toLowerCase();
      return [...(jobsData ?? [])]
        .filter((j) => active.includes(j.status))
        .filter((j) => typeFilter === "all" || j.job_type === typeFilter)
        .filter((j) => presetMatchesJob(preset, j, staleJobIds, reportJobIds))
        .filter((j) => {
          if (!needle) return true;
          return [
            j.id,
            j.job_type,
            j.status,
            j.artifact_id ?? "",
            j.requested_by,
          ]
            .join(" ")
            .toLowerCase()
            .includes(needle);
        })
        .sort((a, b) => {
          if (sortKey === "priority_desc") {
            return b.priority - a.priority || b.updated_at - a.updated_at;
          }
          if (sortKey === "created_desc") {
            return b.created_at - a.created_at;
          }
          return b.updated_at - a.updated_at;
        });
    },
    [jobsData, active, typeFilter, preset, staleJobIds, reportJobIds, search, sortKey],
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
          <div className="flex flex-wrap items-center gap-2">
            <select
              aria-label="Job preset"
              value={preset}
              onChange={(e) =>
                updateQueryParam(
                  searchParams,
                  setSearchParams,
                  "preset",
                  e.currentTarget.value,
                  "all",
                )
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {PRESET_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
            <select
              aria-label="Job type filter"
              value={typeFilter}
              onChange={(e) =>
                updateQueryParam(
                  searchParams,
                  setSearchParams,
                  "type",
                  e.currentTarget.value,
                  "all",
                )
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {TYPE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
            <select
              aria-label="Job sort"
              value={sortKey}
              onChange={(e) =>
                updateQueryParam(
                  searchParams,
                  setSearchParams,
                  "sort",
                  e.currentTarget.value,
                  DEFAULT_SORT,
                )
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {SORT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  Sort: {opt.label}
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
          onChange={(next) =>
            updateQueryListParam(
              searchParams,
              setSearchParams,
              "status",
              next,
              ALL_STATUSES,
            )
          }
          toneFor={(s) => jobTone(s as JobStatus)}
          ariaLabel="Job status filter"
        />
        <div className="mb-3 max-w-md">
          <Input
            aria-label="Search jobs"
            value={search}
            onChange={(event) =>
              updateQueryParam(
                searchParams,
                setSearchParams,
                "q",
                event.currentTarget.value,
                "",
              )
            }
            placeholder="Search job, artifact, requester"
          />
        </div>
        {filtered.length === 0 ? (
          <StateEmpty
            message={
              (jobsData ?? []).length === 0
                ? "No jobs yet. Create an export, backtest, or sweep job."
                : "No jobs match the selected filters."
            }
          />
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
                        state={{ returnTo: returnToJobs }}
                        title={job.id}
                        className="whitespace-nowrap font-mono text-[11px] hover:underline"
                      >
                        {truncateMiddle(job.id, 8, 4)}
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
                          title={job.artifact_id}
                          className="whitespace-nowrap font-mono text-[11px] hover:underline"
                        >
                          {truncateMiddle(job.artifact_id, 14, 8)}
                        </Link>
                      ) : (
                        <span className="text-muted">-</span>
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

function presetMatchesJob(
  preset: JobPreset,
  job: { status: JobStatus; id: string },
  staleJobIds: Set<string>,
  reportJobIds: Set<string>,
): boolean {
  if (preset === "all") return true;
  if (preset === "active") return !isJobTerminal(job.status);
  if (preset === "attention") {
    return (
      ["blocked", "failed", "retryable"].includes(job.status) ||
      staleJobIds.has(job.id)
    );
  }
  if (preset === "completed") return job.status === "completed";
  if (preset === "cancelled") return job.status === "cancelled";
  if (preset === "stale_lease") return staleJobIds.has(job.id);
  return isJobTerminal(job.status) && !reportJobIds.has(job.id);
}
