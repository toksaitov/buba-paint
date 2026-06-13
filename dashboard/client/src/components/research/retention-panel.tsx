import { Banner, StateEmpty } from "../ui/dashboard-primitives";
import { jobTypeLabel } from "../../lib/research-permissions";
import type {
  ResearchRetentionArtifactCandidate,
  ResearchRetentionJobCandidate,
  ResearchRetentionReportCandidate,
  RetentionArchiveResponse,
} from "../../lib/research-types";
import { formatBytes } from "../../lib/utils";

export interface RetentionSelection {
  jobIds: string[];
  reportIds: string[];
  artifactIds: string[];
}

export function RetentionPanel({
  jobs,
  reports,
  artifacts,
  selection,
  setSelection,
}: {
  jobs: ResearchRetentionJobCandidate[];
  reports: ResearchRetentionReportCandidate[];
  artifacts: ResearchRetentionArtifactCandidate[];
  selection: RetentionSelection;
  setSelection: (selection: RetentionSelection) => void;
}) {
  return (
    <div className="space-y-3">
      <RetentionCandidateGroup
        title="Scratch DBs"
        empty="No completed job scratch candidates."
        rows={jobs}
        selectedIds={selection.jobIds}
        onSelect={(jobIds) => setSelection({ ...selection, jobIds })}
        idFor={(candidate) => candidate.job.id}
        labelFor={(candidate) => candidate.job.id}
        subFor={(candidate) =>
          `${jobTypeLabel(candidate.job.job_type)} · ${formatBytes(candidate.scratch_bytes)}`
        }
        eligibleFor={(candidate) => candidate.eligible}
      />
      <RetentionCandidateGroup
        title="Reports"
        empty="No report archive candidates."
        rows={reports}
        selectedIds={selection.reportIds}
        onSelect={(reportIds) => setSelection({ ...selection, reportIds })}
        idFor={(candidate) => candidate.report.id}
        labelFor={(candidate) => candidate.report.title}
        subFor={(candidate) => formatBytes(candidate.bytes)}
        eligibleFor={(candidate) => candidate.eligible}
      />
      <RetentionCandidateGroup
        title="Artifacts"
        empty="No artifact archive candidates."
        rows={artifacts}
        selectedIds={selection.artifactIds}
        onSelect={(artifactIds) =>
          setSelection({ ...selection, artifactIds })
        }
        idFor={(candidate) => candidate.artifact.id}
        labelFor={(candidate) => candidate.artifact.id}
        subFor={(candidate) =>
          `${formatBytes(candidate.bytes)} · ${candidate.active_dependency_count} active deps`
        }
        eligibleFor={(candidate) => candidate.eligible}
      />
    </div>
  );
}

function RetentionCandidateGroup<T>({
  title,
  empty,
  rows,
  selectedIds,
  onSelect,
  idFor,
  labelFor,
  subFor,
  eligibleFor,
}: {
  title: string;
  empty: string;
  rows: T[];
  selectedIds: string[];
  onSelect: (ids: string[]) => void;
  idFor: (row: T) => string;
  labelFor: (row: T) => string;
  subFor: (row: T) => string;
  eligibleFor: (row: T) => boolean;
}) {
  const eligibleRows = rows.filter(eligibleFor);
  if (eligibleRows.length === 0) {
    return (
      <div>
        <div className="mb-1 text-[12px] font-semibold">{title}</div>
        <StateEmpty message={empty} />
      </div>
    );
  }
  return (
    <div>
      <div className="mb-1 text-[12px] font-semibold">{title}</div>
      <div className="space-y-1.5">
        {eligibleRows.map((row) => {
          const id = idFor(row);
          const checked = selectedIds.includes(id);
          return (
            <label
              key={id}
              className="flex items-start gap-2 border border-border px-2 py-1.5 text-[12px]"
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={(event) => {
                  const next = event.currentTarget.checked
                    ? [...selectedIds, id]
                    : selectedIds.filter((value) => value !== id);
                  onSelect(next);
                }}
                className="mt-0.5"
              />
              <span className="min-w-0">
                <span className="block truncate font-mono text-[11px]">
                  {labelFor(row)}
                </span>
                <span className="block text-[11px] text-muted">
                  {subFor(row)}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </div>
  );
}

export function RetentionResult({
  result,
}: {
  result: RetentionArchiveResponse;
}) {
  const archived =
    result.jobs.filter((row) => row.status === "archived").length +
    result.reports.filter((row) => row.status === "archived").length +
    result.artifacts.filter((row) => row.status === "archived").length;
  const errors =
    result.jobs.filter((row) => row.status === "error").length +
    result.reports.filter((row) => row.status === "error").length +
    result.artifacts.filter((row) => row.status === "error").length;
  return (
    <Banner
      tone={errors > 0 ? "warning" : "success"}
      title="Retention archive complete"
    >
      Archived {archived} item{archived === 1 ? "" : "s"}; {errors} error
      {errors === 1 ? "" : "s"}.
    </Banner>
  );
}
