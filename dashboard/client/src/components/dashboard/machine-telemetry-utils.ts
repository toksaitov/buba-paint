import type {
  MachineHostIdentity,
  MachineSample,
  MachineSamplerHealth,
} from "../../lib/types";

export interface MachineTelemetryData {
  host: MachineHostIdentity;
  current: MachineSample | null;
  history: MachineSample[];
  sampler: MachineSamplerHealth;
}

export interface MachineTelemetryWarning {
  tone: "warning" | "danger";
  action: string;
}

const GIB = 1024 * 1024 * 1024;

function rollingAverage(values: number[], window: number): number {
  if (values.length === 0) return 0;
  const slice = values.slice(-window);
  return slice.reduce((a, b) => a + b, 0) / slice.length;
}

export function formatMachineDuration(ms: number): string {
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

export function buildHostResourceWarnings(
  data: MachineTelemetryData,
): MachineTelemetryWarning[] {
  const out: MachineTelemetryWarning[] = [];
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
  if (data.sampler.last_error) {
    out.push({
      tone: "warning",
      action: `Sampler error: ${data.sampler.last_error}`,
    });
  }
  return out;
}
