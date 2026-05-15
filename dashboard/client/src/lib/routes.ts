import type { LucideIcon } from "lucide-react";
import {
  ArrowLeftRight,
  BarChart3,
  Cpu,
  LayoutDashboard,
  LineChart,
  Radio,
  ScrollText,
  Settings2,
  ShieldAlert,
} from "lucide-react";

export type PageScope = "mixed" | "shadow" | "execution" | "operations";

export interface DashboardRouteMeta {
  to: string;
  label: string;
  section: "Monitor" | "Analysis";
  scope: PageScope;
  showContextStrip: boolean;
  contextTitle: string;
  contextDescription: string;
  icon: LucideIcon;
}

export const dashboardRoutes: DashboardRouteMeta[] = [
  {
    to: "/",
    label: "Overview",
    section: "Monitor",
    scope: "mixed",
    showContextStrip: true,
    contextTitle: "Overview",
    contextDescription: "Simulated performance and a quick look at the Polymarket account. Open Execution for venue detail.",
    icon: LayoutDashboard,
  },
  {
    to: "/execution",
    label: "Execution",
    section: "Monitor",
    scope: "execution",
    showContextStrip: true,
    contextTitle: "Execution",
    contextDescription: "Live venue state. Switch to Overview for simulated results.",
    icon: ShieldAlert,
  },
  {
    to: "/logs",
    label: "Logs",
    section: "Monitor",
    scope: "operations",
    showContextStrip: true,
    contextTitle: "Logs",
    contextDescription: "Raw bot logs. Filter by severity, source, or event type.",
    icon: ScrollText,
  },
  {
    to: "/parameters",
    label: "Parameters",
    section: "Monitor",
    scope: "operations",
    showContextStrip: true,
    contextTitle: "Parameters",
    contextDescription: "Read-only snapshot of what the bot was launched with. Not editable from here.",
    icon: Settings2,
  },
  {
    to: "/machine",
    label: "Machine",
    section: "Monitor",
    scope: "operations",
    showContextStrip: true,
    contextTitle: "Machine",
    contextDescription: "Host CPU, memory, swap, disk, and runtime DB. Agent host view.",
    icon: Cpu,
  },
  {
    to: "/equity",
    label: "Trend",
    section: "Analysis",
    scope: "shadow",
    showContextStrip: false,
    contextTitle: "Trend",
    contextDescription: "",
    icon: LineChart,
  },
  {
    to: "/trades",
    label: "Trades",
    section: "Analysis",
    scope: "shadow",
    showContextStrip: false,
    contextTitle: "Trades",
    contextDescription: "",
    icon: ArrowLeftRight,
  },
  {
    to: "/signals",
    label: "Signals",
    section: "Analysis",
    scope: "shadow",
    showContextStrip: false,
    contextTitle: "Signals",
    contextDescription: "",
    icon: Radio,
  },
  {
    to: "/strategies",
    label: "Strategies",
    section: "Analysis",
    scope: "shadow",
    showContextStrip: false,
    contextTitle: "Strategies",
    contextDescription: "",
    icon: BarChart3,
  },
];

export function routeMetaForPath(pathname: string): DashboardRouteMeta {
  if (pathname === "/live" || pathname === "/trading") {
    return dashboardRoutes[1];
  }
  if (pathname === "/stats") {
    return (
      dashboardRoutes.find((r) => r.to === "/strategies") ?? dashboardRoutes[0]
    );
  }
  return (
    dashboardRoutes.find((route) =>
      route.to === "/"
        ? pathname === "/"
        : pathname === route.to || pathname.startsWith(`${route.to}/`),
    ) ?? dashboardRoutes[0]
  );
}
