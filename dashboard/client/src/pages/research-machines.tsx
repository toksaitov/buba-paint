import { useMemo } from "react";
import { Link, useLocation, useSearchParams } from "react-router-dom";
import {
  Banner,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { StatusFilter } from "../components/research/status-filter";
import {
  useResearchMachines,
  useResearchMachineTelemetry,
} from "../hooks/use-research-machines";
import { useRememberResearchListReturn } from "../hooks/use-research-return-to";
import { machineTone } from "../lib/research-permissions";
import { humanize } from "../lib/utils";
import type {
  MachineStatus,
  ResearchMachine,
} from "../lib/research-types";
import {
  readEnumListParam,
  updateQueryListParam,
} from "../lib/research-list-url-state";

const ALL_STATUSES: MachineStatus[] = [
  "not_configured",
  "configured",
  "online",
  "idle",
  "busy",
  "degraded",
  "error",
  "disabled",
  "unreachable",
  "maintenance",
];

export function ResearchMachinesPage() {
  const machinesQuery = useResearchMachines();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  useRememberResearchListReturn("machines", "/research/machines");
  const returnToMachines = `${location.pathname}${location.search}`;
  const active = readEnumListParam(
    searchParams,
    "status",
    ALL_STATUSES,
    ALL_STATUSES,
  );

  const filtered = useMemo(
    () =>
      (machinesQuery.data?.machines ?? []).filter(
        (m) => m.role === "research" && active.includes(m.status),
      ),
    [machinesQuery.data?.machines, active],
  );

  if (machinesQuery.isLoading) {
    return <Loading label="Loading research hosts" />;
  }
  if (machinesQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load research hosts">
        {(machinesQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard title="Research hosts">
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
          toneFor={(s) => machineTone(s as MachineStatus)}
          ariaLabel="Research host status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No research hosts match the selected filters." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">Host</th>
                  <th className="px-2 py-1.5 font-semibold">Machine status</th>
                  <th className="px-2 py-1.5 font-semibold">Telemetry</th>
                  <th className="px-2 py-1.5 font-semibold">Worker</th>
                  <th className="px-2 py-1.5 font-semibold">References</th>
                  <th className="px-2 py-1.5 font-semibold">Last heartbeat</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((machine) => (
                  <ResearchHostRow
                    key={machine.id}
                    machine={machine}
                    returnTo={returnToMachines}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
    </div>
  );
}

function ResearchHostRow({
  machine,
  returnTo,
}: {
  machine: ResearchMachine;
  returnTo: string;
}) {
  const telemetryQuery = useResearchMachineTelemetry(machine.id, true, 20_000);
  const telemetry = telemetryQuery.data;
  const state = telemetry?.telemetry;
  const sampleCount = telemetry?.samples.length ?? 0;
  const dependencyTotal = telemetry
    ? telemetry.dependencies.artifacts +
      telemetry.dependencies.transfers_as_source +
      telemetry.dependencies.transfers_as_destination +
      telemetry.dependencies.active_transfers +
      telemetry.dependencies.jobs_using_source_artifacts +
      telemetry.dependencies.reports_using_source_artifacts
    : null;
  const telemetryTone =
    telemetryQuery.isError || state?.last_error
      ? "danger"
      : !state
        ? "muted"
        : telemetry?.stale || telemetry?.disabled
        ? "warning"
        : "success";
  const telemetryLabel = telemetryQuery.isLoading
    ? "Loading"
    : telemetryQuery.isError
      ? "Error"
      : !state
        ? "Missing"
      : telemetry?.disabled
        ? "Disabled"
        : telemetry?.stale
          ? "Stale"
          : "Healthy";

  return (
    <tr className="border-b border-border last:border-b-0 hover:bg-surface">
      <td className="px-2 py-1.5">
        <Link
          to={`/research/machines/${encodeURIComponent(machine.id)}`}
          state={{ returnTo }}
          className="font-mono text-[11px] hover:underline"
        >
          {machine.id}
        </Link>
        <div className="text-[11px] text-muted">{machine.name}</div>
      </td>
      <td className="px-2 py-1.5">
        <StatusChip
          label={humanize(machine.status)}
          tone={machineTone(machine.status)}
          compact
        />
      </td>
      <td className="px-2 py-1.5">
        <div className="flex flex-wrap items-center gap-1.5">
          <StatusChip label={telemetryLabel} tone={telemetryTone} compact />
          {sampleCount > 0 && (
            <span className="text-[11px] text-muted tabular-nums">
              {sampleCount} samples
            </span>
          )}
        </div>
      </td>
      <td className="px-2 py-1.5 text-muted">
        {state ? (
          <span className="font-mono text-[11px]">
            {state.worker_id} / {humanize(state.worker_status)}
          </span>
        ) : (
          "-"
        )}
      </td>
      <td className="px-2 py-1.5 text-muted tabular-nums">
        {dependencyTotal == null ? "-" : dependencyTotal}
      </td>
      <td className="px-2 py-1.5 text-muted">
        {state ? <RelativeTime epochMs={state.last_heartbeat_ms} /> : "-"}
      </td>
    </tr>
  );
}
