import { useOutletContext } from "react-router-dom";
import { Banner, KeyValueList, SectionCard, StateEmpty } from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
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
import {
  MachineTimeline,
  type TimelineSeries,
} from "../components/dashboard/machine-timeline";
import { useMachine } from "../hooks/use-machine";
import { useTheme } from "../hooks/use-theme";
import { getChartColors } from "../lib/chart-colors";
import { formatBytes } from "../lib/utils";
import type { MachineResponse } from "../lib/types";

const MIB = 1024 * 1024;

function dbTone(walBytes: number | null): ChipTone {
  if (walBytes == null) return "neutral";
  if (walBytes >= GIB) return "warning";
  return "neutral";
}

const GIB = 1024 * 1024 * 1024;

type ChipTone = "neutral" | "muted" | "success" | "warning" | "danger";

function buildWarnings(data: MachineResponse): MachineTelemetryWarning[] {
  const out = buildHostResourceWarnings(data);
  if ((data.runtime_db.wal_bytes ?? 0) >= GIB) {
    out.push({
      tone: "warning",
      action: "WAL exceeds 1 GiB. Investigate writer / checkpoint.",
    });
  }
  return out;
}

export function MachinePage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data, isLoading, isError, error } = useMachine(botId);
  const { theme } = useTheme();
  const colors = getChartColors(theme);

  if (isLoading) return <Loading label="Loading machine status" />;
  if (isError) {
    return (
      <Banner tone="danger" title="Unable to load machine status">
        {error instanceof Error ? error.message : "Try refreshing the page."}
      </Banner>
    );
  }
  if (!data) {
    return (
      <SectionCard title="Machine">
        <StateEmpty message="No machine response yet." />
      </SectionCard>
    );
  }

  const telemetryData: MachineTelemetryData = data;
  const warnings = buildWarnings(data);

  return (
    <div className="space-y-3">
      <MachineWarningBanner warnings={warnings} />

      <HostCard data={telemetryData} startedAtMs={data.agent_started_at_ms} />
      {data.current ? (
        <>
          <CpuCard
            data={telemetryData}
            current={data.current}
            colors={colors}
          />
          <MemoryCard data={telemetryData} current={data.current} colors={colors} />
          <DiskCard data={telemetryData} current={data.current} colors={colors} />
        </>
      ) : (
        <MachineSamplesEmpty
          title="Live samples"
          message="Sampler has not produced a snapshot yet. The first sample arrives within five seconds of agent startup."
        />
      )}
      <RuntimeDbCard data={data} colors={colors} />
    </div>
  );
}

function RuntimeDbCard({
  data,
  colors,
}: {
  data: MachineResponse;
  colors: ReturnType<typeof getChartColors>;
}) {
  const tone = dbTone(data.runtime_db.wal_bytes);
  const series: TimelineSeries[] = [
    {
      label: "DB total",
      dataKey: "total",
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value:
          ((data.runtime_db.db_bytes ?? 0) +
            (data.runtime_db.wal_bytes ?? 0) +
            (data.runtime_db.shm_bytes ?? 0)) /
          MIB,
      })),
      color: colors.lineColor,
      emphasize: true,
      yAxisFormat: (v) => `${v.toFixed(0)}M`,
    },
  ];
  const items = [
    { label: "Path", value: <span className="font-mono text-[11px]">{data.runtime_db.db_path}</span> },
    { label: "paint.db", value: <span className="tabular-nums">{formatBytes(data.runtime_db.db_bytes)}</span> },
    {
      label: "paint.db-wal",
      value: <span className="tabular-nums">{formatBytes(data.runtime_db.wal_bytes)}</span>,
      tone,
    },
    { label: "paint.db-shm", value: <span className="tabular-nums">{formatBytes(data.runtime_db.shm_bytes)}</span> },
  ];
  return (
    <SectionCard title="Runtime DB">
      <div className="space-y-3">
        <KeyValueList items={items} />
        <MachineTimeline
          series={series}
          height={140}
          ariaLabel="DB size history, last 5 minutes"
        />
      </div>
    </SectionCard>
  );
}
