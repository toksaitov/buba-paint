import { Link, useParams } from "react-router-dom";
import {
  Banner,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { JsonViewer } from "../components/research/json-viewer";
import {
  CpuCard,
  DiskCard,
  HostCard,
  MachineSamplesEmpty,
  MachineWarningBanner,
  MemoryCard,
} from "../components/dashboard/machine-telemetry";
import {
  buildHostResourceWarnings,
  type MachineTelemetryData,
  type MachineTelemetryWarning,
} from "../components/dashboard/machine-telemetry-utils";
import { useResearchMachineTelemetry } from "../hooks/use-research-machines";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useTheme } from "../hooks/use-theme";
import { getChartColors } from "../lib/chart-colors";
import { machineTone } from "../lib/research-permissions";
import type { MachineTelemetryResponse } from "../lib/research-types";
import { humanize } from "../lib/utils";

export function ResearchMachineDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const telemetryQuery = useResearchMachineTelemetry(id);
  const { theme } = useTheme();
  const colors = getChartColors(theme);

  if (telemetryQuery.isLoading) {
    return <Loading label="Loading research host" />;
  }
  if (telemetryQuery.isError || !telemetryQuery.data) {
    return (
      <Banner tone="danger" title="Could not load research host">
        {(telemetryQuery.error as Error)?.message ?? "Research host not found."}
      </Banner>
    );
  }

  const data = telemetryQuery.data;
  const machine = data.machine;
  const state = data.telemetry;

  if (machine.role !== "research") {
    return (
      <div className="space-y-3">
        <BackLink />
        <Banner tone="warning" title="Not a research host">
          Machine {machine.id} is {humanize(machine.role)} provenance. Research
          host telemetry pages only open research-role machines.
        </Banner>
        <IdentityCard data={data} />
      </div>
    );
  }

  const telemetryData =
    state?.host && state.sampler
      ? ({
          host: state.host,
          sampler: state.sampler,
          current: data.samples.at(-1) ?? null,
          history: data.samples,
        } satisfies MachineTelemetryData)
      : null;
  const warnings = buildResearchWarnings(data, telemetryData);

  return (
    <div className="space-y-3">
      <BackLink />
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-[14px] font-semibold">{machine.name}</span>
        <span className="font-mono text-[11px] text-muted">{machine.id}</span>
        <StatusChip
          label={humanize(machine.status)}
          tone={machineTone(machine.status)}
        />
        <StatusChip
          label={telemetryStateLabel(data)}
          tone={telemetryStateTone(data)}
        />
      </div>

      <MachineWarningBanner warnings={warnings} />

      {data.disabled && (
        <Banner tone="warning" title="Research host disabled">
          The worker reports telemetry, but this host will not claim new work
          while disabled.
        </Banner>
      )}

      {data.stale && state && (
        <Banner tone="warning" title="Telemetry stale">
          Last heartbeat is older than the configured stale threshold.
        </Banner>
      )}

      <IdentityCard data={data} />

      {telemetryData && state ? (
        <>
          <HostCard
            data={telemetryData}
            startedAtMs={state.last_heartbeat_ms}
            startedLabel="Last heartbeat"
            extraItems={[
              {
                label: "Worker",
                value: (
                  <span className="font-mono text-[11px]">
                    {state.worker_id}
                  </span>
                ),
              },
              {
                label: "Worker status",
                value: humanize(state.worker_status),
              },
            ]}
          />
          <WorkerCard data={data} />
          {telemetryData.current ? (
            <>
              <CpuCard
                data={telemetryData}
                current={telemetryData.current}
                colors={colors}
              />
              <MemoryCard
                data={telemetryData}
                current={telemetryData.current}
                colors={colors}
              />
              <DiskCard
                data={telemetryData}
                current={telemetryData.current}
                colors={colors}
                mountSuffix="research work root"
              />
            </>
          ) : (
            <MachineSamplesEmpty message="Telemetry state exists, but no host samples are available yet." />
          )}
        </>
      ) : (
        <SectionCard title="Telemetry">
          <StateEmpty message="No typed host telemetry has been recorded for this research host yet." />
        </SectionCard>
      )}
    </div>
  );
}

function BackLink() {
  const returnToMachines = useResearchReturnTo("machines", "/research/machines");
  return (
    <Link
      to={returnToMachines}
      className="text-[12px] text-muted hover:underline"
    >
      Research hosts
    </Link>
  );
}

function IdentityCard({ data }: { data: MachineTelemetryResponse }) {
  const machine = data.machine;
  return (
    <SectionCard title="Identity">
      <KeyValueList
        columns={2}
        items={[
          { label: "ID", value: <span className="font-mono">{machine.id}</span> },
          { label: "Name", value: machine.name },
          { label: "Role", value: humanize(machine.role) },
          {
            label: "SSH alias",
            value:
              machine.ssh_alias == null ? (
                <span className="text-muted">-</span>
              ) : (
                <span className="font-mono text-[11px]">{machine.ssh_alias}</span>
              ),
          },
          { label: "Status", value: humanize(machine.status) },
          { label: "Created", value: <RelativeTime epochMs={machine.created_at} /> },
          { label: "Updated", value: <RelativeTime epochMs={machine.updated_at} /> },
        ]}
      />
    </SectionCard>
  );
}

function WorkerCard({ data }: { data: MachineTelemetryResponse }) {
  const state = data.telemetry;
  const deps = data.dependencies;
  return (
    <SectionCard title="Worker">
      {state == null ? (
        <StateEmpty message="No worker heartbeat has reported typed telemetry yet." />
      ) : (
        <div className="space-y-3">
          <KeyValueList
            columns={2}
            items={[
              { label: "Worker ID", value: <span className="font-mono">{state.worker_id}</span> },
              { label: "Worker status", value: humanize(state.worker_status) },
              { label: "Worker version", value: state.worker_version ?? "-" },
              { label: "Last heartbeat", value: <RelativeTime epochMs={state.last_heartbeat_ms} /> },
              { label: "Last sample", value: state.last_sample_ms == null ? "-" : <RelativeTime epochMs={state.last_sample_ms} /> },
              { label: "Stale threshold", value: `${Math.round(data.stale_after_ms / 1000)}s` },
              { label: "Artifacts", value: String(deps.artifacts) },
              { label: "Transfers as source", value: String(deps.transfers_as_source) },
              { label: "Transfers as destination", value: String(deps.transfers_as_destination) },
              { label: "Active transfers", value: String(deps.active_transfers) },
              { label: "Jobs using artifacts", value: String(deps.jobs_using_source_artifacts) },
              { label: "Reports using artifacts", value: String(deps.reports_using_source_artifacts) },
            ]}
          />
          <JsonViewer
            value={state.activity ?? null}
            label="Worker activity"
            emptyLabel="No worker activity payload recorded."
            maxHeight={240}
          />
        </div>
      )}
    </SectionCard>
  );
}

function buildResearchWarnings(
  data: MachineTelemetryResponse,
  telemetryData: MachineTelemetryData | null,
): MachineTelemetryWarning[] {
  const out = telemetryData ? buildHostResourceWarnings(telemetryData) : [];
  if (!data.telemetry) {
    out.push({
      tone: "warning",
      action: "No typed research-host telemetry has been recorded yet.",
    });
  }
  if (!telemetryData && data.telemetry?.sampler?.last_error) {
    out.push({
      tone: "warning",
      action: `Sampler error: ${data.telemetry.sampler.last_error}`,
    });
  }
  if (data.stale && data.telemetry) {
    out.push({
      tone: "warning",
      action: "Research worker heartbeat is stale.",
    });
  }
  if (data.disabled) {
    out.push({
      tone: "warning",
      action: "Research host is disabled and will not claim work.",
    });
  }
  if (data.telemetry?.last_error) {
    out.push({
      tone: "danger",
      action: `Worker error: ${data.telemetry.last_error}`,
    });
  }
  return out;
}

function telemetryStateLabel(data: MachineTelemetryResponse): string {
  if (data.disabled) return "Disabled";
  if (!data.telemetry) return "Missing telemetry";
  if (data.stale) return "Stale";
  if (data.telemetry.last_error) return "Error";
  return "Telemetry healthy";
}

function telemetryStateTone(
  data: MachineTelemetryResponse,
): "neutral" | "muted" | "success" | "warning" | "danger" {
  if (data.telemetry?.last_error) return "danger";
  if (!data.telemetry) return "muted";
  if (data.disabled || data.stale) return "warning";
  return "success";
}
