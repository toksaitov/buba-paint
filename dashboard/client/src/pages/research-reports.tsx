import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
  Banner,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchReports } from "../hooks/use-research-reports";
import { reportTone } from "../lib/research-permissions";
import type { ReportStatus } from "../lib/research-types";
import { humanize } from "../lib/utils";

const ALL_STATUSES: ReportStatus[] = ["available", "archived"];

export function ResearchReportsPage() {
  const reportsQuery = useResearchReports();
  const [active, setActive] = useState<string[]>(["available"]);

  const reportsData = reportsQuery.data?.reports;
  const filtered = useMemo(
    () =>
      [...(reportsData ?? [])]
        .filter((r) => active.includes(r.status))
        .sort((a, b) => b.updated_at - a.updated_at),
    [reportsData, active],
  );

  if (reportsQuery.isLoading) {
    return <Loading label="Loading reports" />;
  }
  if (reportsQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load reports">
        {(reportsQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard title="All reports">
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => reportTone(s as ReportStatus)}
          ariaLabel="Report status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No reports yet — they appear after a backtest or sweep job completes." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">Title</th>
                  <th className="px-2 py-1.5 font-semibold">Job</th>
                  <th className="px-2 py-1.5 font-semibold">Artifact</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((report) => (
                  <tr
                    key={report.id}
                    className="border-b border-border last:border-b-0 hover:bg-surface"
                  >
                    <td className="px-2 py-1.5">
                      <Link
                        to={`/research/reports/${encodeURIComponent(report.id)}`}
                        className="font-semibold hover:underline"
                      >
                        {report.title}
                      </Link>
                      <div className="font-mono text-[11px] text-muted">
                        {report.id}
                      </div>
                    </td>
                    <td className="px-2 py-1.5">
                      <Link
                        to={`/research/jobs/${encodeURIComponent(report.job_id)}`}
                        className="font-mono text-[11px] hover:underline"
                      >
                        {report.job_id}
                      </Link>
                    </td>
                    <td className="px-2 py-1.5">
                      {report.artifact_id ? (
                        <Link
                          to={`/research/artifacts/${encodeURIComponent(report.artifact_id)}`}
                          className="font-mono text-[11px] hover:underline"
                        >
                          {report.artifact_id}
                        </Link>
                      ) : (
                        <span className="text-muted">—</span>
                      )}
                    </td>
                    <td className="px-2 py-1.5">
                      <StatusChip
                        label={humanize(report.status)}
                        tone={reportTone(report.status)}
                        compact
                      />
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      <RelativeTime epochMs={report.updated_at} />
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
