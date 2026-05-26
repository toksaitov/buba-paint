export function cn(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}

export function formatUsd(value: number): string {
  return value.toLocaleString("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

export function formatSignedUsd(value: number): string {
  return `${value >= 0 ? "+" : "-"}${formatUsd(Math.abs(value))}`;
}

export function formatPct(value: number): string {
  return `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
}

function pad2(value: number): string {
  return value.toString().padStart(2, "0");
}

function localDateTime(epochMs: number): Date {
  return new Date(epochMs);
}

export function localTimeZoneLabel(epochMs = Date.now()): string {
  const part = new Intl.DateTimeFormat(undefined, { timeZoneName: "short" })
    .formatToParts(new Date(epochMs))
    .find((item) => item.type === "timeZoneName");
  return part?.value ?? "";
}

function formatLocalDateTimeParts(epochMs: number, includeSeconds: boolean): string {
  const date = localDateTime(epochMs);
  const base = `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
  const time = `${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
  if (!includeSeconds) {
    return `${base} ${time}`;
  }
  return `${base} ${time}:${pad2(date.getSeconds())}`;
}

export function formatTime(epochMs: number): string {
  const date = localDateTime(epochMs);
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

export function formatDateTime(epochMs: number): string {
  const zone = localTimeZoneLabel(epochMs);
  return `${formatLocalDateTimeParts(epochMs, true)}${zone ? ` ${zone}` : ""}`;
}

export function formatChartTime(epochMs: number): string {
  return formatDateTime(epochMs);
}

export function formatChartTick(epochMs: number): string {
  const date = localDateTime(epochMs);
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
}

export function formatLogTimestamp(epochMs: number): string {
  return formatDateTime(epochMs);
}

export function pnlColor(value: number): string {
  if (value > 0) return "text-accent-green";
  if (value < 0) return "text-accent-red";
  return "text-muted";
}

export function truncateMiddle(value: string, head = 8, tail = 6): string {
  if (value.length <= head + tail + 3) return value;
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[index]}`;
}

export function humanize(value: string): string {
  if (!value) return value;
  const spaced = value.replace(/_/g, " ").trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1).toLowerCase();
}

export function formatDurationShort(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0s";
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}
