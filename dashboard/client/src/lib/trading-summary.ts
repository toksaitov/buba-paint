import type {
  TradingAlert,
  TradingCapabilities,
  TradingHealth,
  TradingSummary,
} from "./types";

export type ChipTone = "neutral" | "muted" | "success" | "warning" | "danger";

export function runtimeModeLabel(mode: string): string {
  switch (mode) {
    case "live_readonly":
      return "Live Readonly";
    case "live_trading":
      return "Live Trading";
    default:
      return "Paper";
  }
}

export function tradingStateLabel(state: string): string {
  switch (state) {
    case "readonly":
      return "Read only";
    case "gated":
      return "Gated";
    case "degraded":
      return "Degraded";
    case "disarmed":
      return "Disarmed";
    case "armed":
      return "Armed";
    case "halted":
      return "Halted";
    case "unknown_order":
      return "Unknown order";
    default:
      return "Paper";
  }
}

export function processStateLabel(state: string): string {
  switch (state) {
    case "running":
      return "Running";
    case "monitoring":
      return "Monitor Only";
    default:
      return "Stopped";
  }
}

export function processStateTone(state: string): ChipTone {
  return state === "running" ? "success" : state === "monitoring" ? "warning" : "danger";
}

export function tradingStateTone(state: string): ChipTone {
  switch (state) {
    case "readonly":
      return "neutral";
    case "degraded":
      return "warning";
    case "halted":
    case "unknown_order":
    case "armed":
      return "danger";
    case "gated":
      return "muted";
    default:
      return "muted";
  }
}

export function healthTone(health: TradingHealth): ChipTone {
  switch (health.state) {
    case "healthy":
      return "success";
    case "critical":
      return "danger";
    case "warning":
      return "warning";
    default:
      return "muted";
  }
}

export function capabilityEntries(capabilities: TradingCapabilities) {
  return [
    { key: "preflight", label: "Preflight", capability: capabilities.preflight },
    { key: "arm", label: "Arm", capability: capabilities.arm },
    { key: "disarm", label: "Disarm", capability: capabilities.disarm },
    { key: "cancel_all", label: "Cancel All", capability: capabilities.cancel_all },
    {
      key: "stop_after_flat",
      label: "Stop After Flat",
      capability: capabilities.stop_after_flat,
    },
    { key: "redeem", label: "Redeem", capability: capabilities.redeem },
    { key: "kill_switch", label: "Kill Switch", capability: capabilities.kill_switch },
  ];
}

export function highestAlertTone(alerts: TradingAlert[]): ChipTone {
  if (alerts.some((alert) => alert.severity === "critical")) return "danger";
  if (alerts.some((alert) => alert.severity === "warning")) return "warning";
  return "muted";
}

export function alertSummaryLabel(summary: TradingSummary): string {
  if (summary.alerts.length === 0) return "No Alerts";
  if (summary.alerts.some((alert) => alert.severity === "critical")) {
    return `${summary.alerts.length} Alert${summary.alerts.length === 1 ? "" : "s"}`;
  }
  return `${summary.alerts.length} Notice${summary.alerts.length === 1 ? "" : "s"}`;
}

export function mobileHeaderStateLabel(runtime: string, tradingState: string): string {
  if (runtime === "paper") return "Paper";
  const runtimeLabel = runtime === "live_trading" ? "Live" : "Live";
  const state = tradingStateLabel(tradingState);
  return `${runtimeLabel} ${state}`;
}
