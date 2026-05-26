import { useOutletContext } from "react-router-dom";
import {
  Banner,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StateEmpty,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { DonutGauge, type DonutTone } from "../components/dashboard/donut-gauge";
import {
  MachineTimeline,
  type TimelineSeries,
} from "../components/dashboard/machine-timeline";
import { useMachine } from "../hooks/use-machine";
import { useTheme } from "../hooks/use-theme";
import { getChartColors } from "../lib/chart-colors";
import { formatBytes } from "../lib/utils";
import type { MachineResponse, MachineSample } from "../lib/types";

type ChipTone = "neutral" | "muted" | "success" | "warning" | "danger";

const GIB = 1024 * 1024 * 1024;
const MIB = 1024 * 1024;

function formatPercent(used: number, total: number): string {
  if (total <= 0) return "—";
  return `${((used / total) * 100).toFixed(1)} %`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const minutes = Math.floor(secs / 60);
  if (minutes < 60) return `${minutes}m ${secs % 60}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

function rollingAverage(values: number[], window: number): number {
  if (values.length === 0) return 0;
  const slice = values.slice(-window);
  return slice.reduce((a, b) => a + b, 0) / slice.length;
}

function cpuTone(percent: number, history: MachineSample[]): ChipTone {
  const recent = rollingAverage(
    history.map((s) => s.cpu_percent),
    5,
  );
  const peak = Math.max(percent, recent);
  if (peak >= 90) return "danger";
  if (peak >= 70) return "warning";
  return "neutral";
}

function memTone(used: number, total: number): ChipTone {
  if (total <= 0) return "neutral";
  const availFrac = (total - used) / total;
  if (availFrac < 0.1) return "danger";
  if (availFrac < 0.2) return "warning";
  return "neutral";
}

function swapTone(used: number, total: number): ChipTone {
  if (total <= 0) return "neutral";
  const frac = used / total;
  if (frac > 0.5) return "danger";
  if (frac > 0.25) return "warning";
  return "neutral";
}

function diskTone(used: number, total: number): ChipTone {
  if (total <= 0) return "neutral";
  const usedFrac = used / total;
  const free = total - used;
  if (usedFrac >= 0.9 || free < 2 * GIB) return "danger";
  if (usedFrac >= 0.8 || free < 5 * GIB) return "warning";
  return "neutral";
}

function dbTone(walBytes: number | null): ChipTone {
  if (walBytes == null) return "neutral";
  if (walBytes >= GIB) return "warning";
  return "neutral";
}

function chipToneToDonutTone(tone: ChipTone): DonutTone {
  if (tone === "danger") return "danger";
  if (tone === "warning") return "warning";
  return "default";
}

interface Warning {
  tone: "warning" | "danger";
  action: string;
}

function buildWarnings(data: MachineResponse): Warning[] {
  const out: Warning[] = [];
  const c = data.current;
  if (c) {
    const free = c.disk_total_bytes - c.disk_used_bytes;
    if (c.disk_total_bytes > 0) {
      if (c.disk_used_bytes / c.disk_total_bytes >= 0.9 || free < 2 * GIB) {
        out.push({
          tone: "danger",
          action: "Free disk before it falls below 2 GiB.",
        });
      } else if (
        c.disk_used_bytes / c.disk_total_bytes >= 0.8 ||
        free < 5 * GIB
      ) {
        out.push({ tone: "warning", action: "Plan disk cleanup soon." });
      }
    }
    if (c.mem_total_bytes > 0) {
      const availFrac = c.mem_available_bytes / c.mem_total_bytes;
      if (availFrac < 0.1) {
        out.push({
          tone: "danger",
          action: "Available memory below 10%. Investigate RAM pressure.",
        });
      } else if (availFrac < 0.2) {
        out.push({
          tone: "warning",
          action: "Available memory below 20%.",
        });
      }
    }
    if (c.swap_total_bytes > 0) {
      const swapFrac = c.swap_used_bytes / c.swap_total_bytes;
      if (swapFrac > 0.5) {
        out.push({
          tone: "danger",
          action: "Swap above 50%. System is paging heavily.",
        });
      } else if (swapFrac > 0.25) {
        out.push({ tone: "warning", action: "Swap above 25%." });
      }
    }
    const cpu5 = rollingAverage(
      data.history.map((s) => s.cpu_percent),
      5,
    );
    if (cpu5 > 90) {
      out.push({
        tone: "danger",
        action: "CPU above 90% on 5-sample average.",
      });
    } else if (cpu5 > 70) {
      out.push({
        tone: "warning",
        action: "CPU above 70% on 5-sample average.",
      });
    }
  }
  if ((data.runtime_db.wal_bytes ?? 0) >= GIB) {
    out.push({
      tone: "warning",
      action: "WAL exceeds 1 GiB. Investigate writer / checkpoint.",
    });
  }
  if (data.sampler.last_error) {
    out.push({
      tone: "warning",
      action: `Sampler error: ${data.sampler.last_error}`,
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

  const warnings = buildWarnings(data);
  const worst: "warning" | "danger" | null = warnings.some(
    (w) => w.tone === "danger",
  )
    ? "danger"
    : warnings.length > 0
      ? "warning"
      : null;

  return (
    <div className="space-y-3">
      {worst && (
        <Banner
          tone={worst}
          title={
            worst === "danger"
              ? "Host needs immediate attention"
              : "Host warnings active"
          }
        >
          <ul className="ml-4 list-disc">
            {warnings.map((w) => (
              <li key={w.action}>{w.action}</li>
            ))}
          </ul>
        </Banner>
      )}

      <HostCard data={data} />
      {data.current ? (
        <>
          <CpuCard
            data={data}
            current={data.current}
            colors={colors}
          />
          <MemoryCard data={data} current={data.current} colors={colors} />
          <DiskCard data={data} current={data.current} colors={colors} />
        </>
      ) : (
        <SectionCard title="Live samples">
          <StateEmpty message="Sampler has not produced a snapshot yet. The first sample arrives within five seconds of agent startup." />
        </SectionCard>
      )}
      <RuntimeDbCard data={data} colors={colors} />
    </div>
  );
}

function HostCard({ data }: { data: MachineResponse }) {
  const items = [
    { label: "Hostname", value: data.host.hostname },
    { label: "OS", value: `${data.host.os_name} ${data.host.os_version}`.trim() },
    { label: "Kernel", value: data.host.kernel_version },
    { label: "CPUs", value: <span className="tabular-nums">{data.host.cpu_count}</span> },
    { label: "Total RAM", value: <span className="tabular-nums">{formatBytes(data.host.total_ram_bytes)}</span> },
    {
      label: "Agent started",
      value: <RelativeTime epochMs={data.agent_started_at_ms} />,
    },
    {
      label: "Sampler interval",
      value: <span className="tabular-nums">{formatDuration(data.sampler.sample_interval_ms)}</span>,
    },
    {
      label: "Samples collected",
      value: <span className="tabular-nums">{data.sampler.samples_collected.toLocaleString()}</span>,
    },
  ];
  return (
    <SectionCard title="Host">
      <KeyValueList items={items} />
    </SectionCard>
  );
}

function CpuCard({
  data,
  current,
  colors,
}: {
  data: MachineResponse;
  current: MachineSample;
  colors: ReturnType<typeof getChartColors>;
}) {
  const tone = cpuTone(current.cpu_percent, data.history);
  const series: TimelineSeries[] = [
    {
      label: "All cores",
      dataKey: "global",
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value: s.cpu_percent,
      })),
      color: colors.lineColor,
      emphasize: true,
      yAxisFormat: (v) => `${v.toFixed(0)}%`,
    },
    ...Array.from({ length: data.host.cpu_count }, (_, i) => ({
      label: `Core ${i}`,
      dataKey: `core_${i}`,
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value: s.per_core_cpu[i] ?? 0,
      })),
      color: colors.seriesPalette[i % colors.seriesPalette.length],
    })),
  ];
  const items = [
    { label: "All cores", value: <span className="tabular-nums">{current.cpu_percent.toFixed(1)} %</span>, tone },
    ...current.per_core_cpu.map((v, i) => ({
      label: `Core ${i}`,
      value: <span className="tabular-nums">{v.toFixed(1)} %</span>,
    })),
    {
      label: "Load 1m",
      value: <span className="tabular-nums">{current.load_one == null ? "—" : current.load_one.toFixed(2)}</span>,
    },
    {
      label: "Load 5m",
      value: <span className="tabular-nums">{current.load_five == null ? "—" : current.load_five.toFixed(2)}</span>,
    },
    {
      label: "Load 15m",
      value: <span className="tabular-nums">{current.load_fifteen == null ? "—" : current.load_fifteen.toFixed(2)}</span>,
    },
  ];
  return (
    <SectionCard
      title="CPU"
      toolbar={
        <DonutGauge
          used={current.cpu_percent}
          total={100}
          label="CPU"
          tone={chipToneToDonutTone(tone)}
          ariaLabel="Current CPU usage"
        />
      }
    >
      <div className="space-y-3">
        <KeyValueList items={items} />
        <MachineTimeline
          series={series}
          yMin={0}
          yMax={100}
          height={200}
          ariaLabel="CPU history, last 5 minutes, all cores"
        />
      </div>
    </SectionCard>
  );
}

function MemoryCard({
  data,
  current,
  colors,
}: {
  data: MachineResponse;
  current: MachineSample;
  colors: ReturnType<typeof getChartColors>;
}) {
  const memToneValue = memTone(current.mem_used_bytes, current.mem_total_bytes);
  const swapToneValue = swapTone(
    current.swap_used_bytes,
    current.swap_total_bytes,
  );
  const series: TimelineSeries[] = [
    {
      label: "Memory used %",
      dataKey: "mem",
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value:
          s.mem_total_bytes > 0
            ? (s.mem_used_bytes / s.mem_total_bytes) * 100
            : 0,
      })),
      color: colors.lineColor,
      emphasize: true,
      yAxisFormat: (v) => `${v.toFixed(0)}%`,
    },
    {
      label: "Swap used %",
      dataKey: "swap",
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value:
          s.swap_total_bytes > 0
            ? (s.swap_used_bytes / s.swap_total_bytes) * 100
            : 0,
      })),
      color: colors.seriesPalette[3],
    },
  ];
  const items = [
    {
      label: "Memory used",
      value: <span className="tabular-nums">{formatBytes(current.mem_used_bytes)} / {formatBytes(current.mem_total_bytes)}</span>,
      tone: memToneValue,
    },
    {
      label: "Available",
      value: <span className="tabular-nums">{formatBytes(current.mem_available_bytes)} ({formatPercent(current.mem_available_bytes, current.mem_total_bytes)})</span>,
    },
    {
      label: "Swap used",
      value: <span className="tabular-nums">{formatBytes(current.swap_used_bytes)} / {formatBytes(current.swap_total_bytes)}</span>,
      tone: swapToneValue,
    },
  ];
  return (
    <SectionCard
      title="Memory & Swap"
      toolbar={
        <div className="flex gap-2">
          <DonutGauge
            used={current.mem_used_bytes}
            total={current.mem_total_bytes}
            label="MEM"
            tone={chipToneToDonutTone(memToneValue)}
            ariaLabel="Current memory usage"
          />
          <DonutGauge
            used={current.swap_used_bytes}
            total={current.swap_total_bytes}
            label="SWAP"
            tone={chipToneToDonutTone(swapToneValue)}
            ariaLabel="Current swap usage"
          />
        </div>
      }
    >
      <div className="space-y-3">
        <KeyValueList items={items} />
        <MachineTimeline
          series={series}
          yMin={0}
          yMax={100}
          height={160}
          ariaLabel="Memory and swap history, last 5 minutes"
        />
      </div>
    </SectionCard>
  );
}

function DiskCard({
  data,
  current,
  colors,
}: {
  data: MachineResponse;
  current: MachineSample;
  colors: ReturnType<typeof getChartColors>;
}) {
  const tone = diskTone(current.disk_used_bytes, current.disk_total_bytes);
  const series: TimelineSeries[] = [
    {
      label: "Disk used %",
      dataKey: "disk",
      values: data.history.map((s) => ({
        ts_ms: s.sampled_at_ms,
        value:
          s.disk_total_bytes > 0
            ? (s.disk_used_bytes / s.disk_total_bytes) * 100
            : 0,
      })),
      color: colors.lineColor,
      emphasize: true,
      yAxisFormat: (v) => `${v.toFixed(0)}%`,
    },
  ];
  const free = current.disk_total_bytes - current.disk_used_bytes;
  const items = [
    { label: "Mount", value: <span className="font-mono text-[11px]">{current.disk_mount} (agent host view)</span> },
    {
      label: "Used",
      value: <span className="tabular-nums">{formatBytes(current.disk_used_bytes)} / {formatBytes(current.disk_total_bytes)}</span>,
      tone,
    },
    {
      label: "Free",
      value: <span className="tabular-nums">{formatBytes(free)} ({formatPercent(free, current.disk_total_bytes)})</span>,
    },
  ];
  return (
    <SectionCard
      title="Disk"
      toolbar={
        <DonutGauge
          used={current.disk_used_bytes}
          total={current.disk_total_bytes}
          label="DISK"
          tone={chipToneToDonutTone(tone)}
          ariaLabel="Current disk usage"
        />
      }
    >
      <div className="space-y-3">
        <KeyValueList items={items} />
        <MachineTimeline
          series={series}
          yMin={0}
          yMax={100}
          height={160}
          ariaLabel="Disk usage history, last 5 minutes"
        />
      </div>
    </SectionCard>
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
