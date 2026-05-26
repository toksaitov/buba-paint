import { Link } from "react-router-dom";
import {
  Banner,
  MetricCard,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchJobs } from "../hooks/use-research-jobs";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useResearchReports } from "../hooks/use-research-reports";
import { useResearchTransfers } from "../hooks/use-research-transfers";
import {
  isJobTerminal,
  isTransferTerminal,
  jobTone,
  jobTypeLabel,
  machineTone,
  transferTone,
} from "../lib/research-permissions";
import { humanize } from "../lib/utils";

function pickRecent<T extends { updated_at: number }>(
  rows: T[],
  count: number,
): T[] {
  return [...rows].sort((a, b) => b.updated_at - a.updated_at).slice(0, count);
}

export function ResearchOverviewPage() {
  const machinesQuery = useResearchMachines();
  const artifactsQuery = useResearchArtifacts();
  const transfersQuery = useResearchTransfers();
  const jobsQuery = useResearchJobs();
  const reportsQuery = useResearchReports();

  if (machinesQuery.isLoading || jobsQuery.isLoading) {
    return <Loading label="Loading research overview" />;
  }
  if (machinesQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load machines">
        {(machinesQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  const machines = machinesQuery.data?.machines ?? [];
  const artifacts = artifactsQuery.data?.artifacts ?? [];
  const transfers = transfersQuery.data?.transfers ?? [];
  const jobs = jobsQuery.data?.jobs ?? [];
  const reports = reportsQuery.data?.reports ?? [];

  const activeJobs = jobs.filter((j) => !isJobTerminal(j.status)).length;
  const queuedJobs = jobs.filter((j) => j.status === "queued").length;
  const runningTransfers = transfers.filter(
    (t) => !isTransferTerminal(t.status) && t.status !== "paused",
  ).length;
  const availableArtifacts = artifacts.filter((a) => a.status === "available")
    .length;

  const recentJobs = pickRecent(jobs, 5);
  const recentTransfers = pickRecent(transfers, 5);
  const recentReports = pickRecent(reports, 3);

  const isEmpty =
    artifacts.length === 0 &&
    transfers.length === 0 &&
    jobs.length === 0 &&
    reports.length === 0;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <MetricCard label="Active jobs" value={activeJobs.toString()} />
        <MetricCard label="Queued jobs" value={queuedJobs.toString()} />
        <MetricCard
          label="Running transfers"
          value={runningTransfers.toString()}
        />
        <MetricCard
          label="Available artifacts"
          value={availableArtifacts.toString()}
        />
      </div>

      <SectionCard title="Machines">
        {machines.length === 0 ? (
          <StateEmpty message="No machines registered." />
        ) : (
          <div className="flex flex-wrap gap-2">
            {machines.map((machine) => (
              <Link
                key={machine.id}
                to={`/research/machines/${encodeURIComponent(machine.id)}`}
                className="inline-flex items-center gap-2 border border-border px-2 py-1 text-[12px] hover:bg-surface"
              >
                <StatusChip
                  label={humanize(machine.status)}
                  tone={machineTone(machine.status)}
                  compact
                />
                <span className="font-semibold">{machine.name}</span>
                <span className="text-muted">{machine.id}</span>
              </Link>
            ))}
          </div>
        )}
      </SectionCard>

      {isEmpty && (
        <Banner tone="info" title="No research activity yet">
          Create an export job to package a live run, or import an existing
          artifact directory.
        </Banner>
      )}

      <div className="grid gap-3 lg:grid-cols-3">
        <SectionCard title="Recent jobs">
          {recentJobs.length === 0 ? (
            <StateEmpty message="No jobs yet." />
          ) : (
            <ul className="space-y-1.5">
              {recentJobs.map((job) => (
                <li key={job.id}>
                  <Link
                    to={`/research/jobs/${encodeURIComponent(job.id)}`}
                    className="flex items-start justify-between gap-2 border border-border px-2 py-1.5 hover:bg-surface"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <StatusChip
                          label={jobTypeLabel(job.job_type)}
                          tone="neutral"
                          compact
                        />
                        <StatusChip
                          label={humanize(job.status)}
                          tone={jobTone(job.status)}
                          compact
                        />
                      </div>
                      <div className="truncate text-[11px] text-muted">
                        {job.id}
                      </div>
                    </div>
                    <RelativeTime epochMs={job.updated_at} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </SectionCard>

        <SectionCard title="Recent transfers">
          {recentTransfers.length === 0 ? (
            <StateEmpty message="No transfers yet." />
          ) : (
            <ul className="space-y-1.5">
              {recentTransfers.map((transfer) => (
                <li key={transfer.id}>
                  <Link
                    to={`/research/transfers/${encodeURIComponent(transfer.id)}`}
                    className="flex items-start justify-between gap-2 border border-border px-2 py-1.5 hover:bg-surface"
                  >
                    <div className="min-w-0">
                      <StatusChip
                        label={humanize(transfer.status)}
                        tone={transferTone(transfer.status)}
                        compact
                      />
                      <div className="truncate text-[11px] text-muted">
                        {transfer.artifact_id}
                      </div>
                    </div>
                    <RelativeTime epochMs={transfer.updated_at} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </SectionCard>

        <SectionCard title="Recent reports">
          {recentReports.length === 0 ? (
            <StateEmpty message="No reports yet." />
          ) : (
            <ul className="space-y-1.5">
              {recentReports.map((report) => (
                <li key={report.id}>
                  <Link
                    to={`/research/reports/${encodeURIComponent(report.id)}`}
                    className="flex items-start justify-between gap-2 border border-border px-2 py-1.5 hover:bg-surface"
                  >
                    <div className="min-w-0">
                      <div className="truncate font-semibold">
                        {report.title}
                      </div>
                      <div className="truncate text-[11px] text-muted">
                        {report.job_id}
                      </div>
                    </div>
                    <RelativeTime epochMs={report.updated_at} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </SectionCard>
      </div>
    </div>
  );
}
