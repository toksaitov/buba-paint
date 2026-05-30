import type { LucideIcon } from "lucide-react";
import {
  ArrowLeftRight,
  ArrowRightLeft,
  BarChart3,
  Cpu,
  FileBarChart2,
  FlaskConical,
  LayoutDashboard,
  LineChart,
  ListTodo,
  Package,
  Radio,
  ScrollText,
  Server,
  Settings2,
  ShieldAlert,
} from "lucide-react";

export type PageScope =
  | "mixed"
  | "shadow"
  | "execution"
  | "operations"
  | "research";

export type RouteSection = "Monitor" | "Analysis" | "Research";

export interface DashboardRouteMeta {
  to: string;
  label: string;
  section: RouteSection;
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
  {
    to: "/research",
    label: "Overview",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Research",
    contextDescription:
      "Central control plane for export, transfer, backtest, and sweep workflows.",
    icon: FlaskConical,
  },
  {
    to: "/research/machines",
    label: "Machines",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Research Hosts",
    contextDescription:
      "Research host telemetry, worker health, and dependency context.",
    icon: Server,
  },
  {
    to: "/research/artifacts",
    label: "Artifacts",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Artifacts",
    contextDescription:
      "Exported run packages and their manifest and checksum status.",
    icon: Package,
  },
  {
    to: "/research/transfers",
    label: "Transfers",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Transfers",
    contextDescription:
      "In-progress and historical artifact transfers between machines.",
    icon: ArrowRightLeft,
  },
  {
    to: "/research/jobs",
    label: "Jobs",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Jobs",
    contextDescription:
      "Export, backtest, and sweep job queue and history.",
    icon: ListTodo,
  },
  {
    to: "/research/reports",
    label: "Reports",
    section: "Research",
    scope: "research",
    showContextStrip: true,
    contextTitle: "Reports",
    contextDescription:
      "Generated backtest and sweep reports with metrics and CSV exports.",
    icon: FileBarChart2,
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
  const exact = dashboardRoutes.find((route) => route.to === pathname);
  if (exact) return exact;
  const prefixMatches = dashboardRoutes
    .filter(
      (route) => route.to !== "/" && pathname.startsWith(`${route.to}/`),
    )
    .sort((a, b) => b.to.length - a.to.length);
  if (prefixMatches.length > 0) return prefixMatches[0];
  if (pathname === "/") {
    return dashboardRoutes.find((r) => r.to === "/") ?? dashboardRoutes[0];
  }
  if (pathname.startsWith("/research")) {
    return (
      dashboardRoutes.find((r) => r.to === "/research") ?? dashboardRoutes[0]
    );
  }
  return dashboardRoutes[0];
}
