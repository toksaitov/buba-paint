import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-report-missing-file" }),
  useNavigate: () => vi.fn(),
  useLocation: () => ({
    pathname: "/research/reports/fixture-report-missing-file",
    search: "",
    state: null,
  }),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReport: vi.fn(),
  useResearchReportJson: vi.fn(),
  useResearchReportCsv: vi.fn(() => ({ isLoading: false, data: null })),
}));

vi.mock("../../hooks/use-theme", () => ({
  useTheme: () => ({ theme: "light" }),
}));

vi.mock("recharts", () => ({
  CartesianGrid: () => createElement("g", { "data-testid": "grid" }),
  Line: () => createElement("path", { "data-testid": "line" }),
  LineChart: ({ children }: { children: ReactNode }) =>
    createElement("div", { "data-testid": "line-chart" }, children),
  ResponsiveContainer: ({ children }: { children: ReactNode }) =>
    createElement("div", { "data-testid": "responsive" }, children),
  Tooltip: () => createElement("div", { "data-testid": "tooltip" }),
  XAxis: () => createElement("div", { "data-testid": "x-axis" }),
  YAxis: () => createElement("div", { "data-testid": "y-axis" }),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchReportDetailPage } from "../research-report-detail";
import {
  useResearchReport,
  useResearchReportJson,
} from "../../hooks/use-research-reports";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureReportAvailable,
  fixtureReportMissingFile,
} from "../../lib/research-fixtures";

const mockUseReport = vi.mocked(useResearchReport);
const mockUseJson = vi.mocked(useResearchReportJson);

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return createElement(QueryClientProvider, { client: qc }, children);
}

beforeEach(() => {
  vi.clearAllMocks();
  useAuthStore.setState({
    token: "tok",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockUseReport.mockReturnValue({
    data: fixtureReportMissingFile(),
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchReport>);
  mockUseJson.mockReturnValue({
    isLoading: false,
    data: undefined,
    isError: false,
  } as ReturnType<typeof useResearchReportJson>);
});

describe("ResearchReportDetailPage - missing file", () => {
  it("links back to source job and artifact for cross-link verification", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("link", { name: /fixture-job-blocked/ }),
    ).toBeInTheDocument();
  });

  it("renders missing-file danger banner when fetch fails", () => {
    mockUseJson.mockReturnValue({
      isLoading: false,
      isError: true,
      error: new Error("not found"),
      data: undefined,
    } as ReturnType<typeof useResearchReportJson>);
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByText(/report files appear to be missing/i),
    ).toBeInTheDocument();
  });

  it("admin sees delete-with-files button", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /delete with files/i }),
    ).toBeInTheDocument();
  });

  it("observer sees mutate buttons disabled", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchReportDetailPage />, { wrapper });
    const archive = screen.getByRole("button", { name: /^archive$/i });
    expect(archive).toBeDisabled();
  });

  it("shows 'Notes/tags: unsupported' explicit placeholder", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByText(/notes\/tags: unsupported/i),
    ).toBeInTheDocument();
  });

  it("renders schema v2 metrics, charts, and rejection diagnostics", () => {
    const report = {
      ...fixtureReportAvailable(),
      summary_json: JSON.stringify({
        schema_version: 2,
        provenance: {
          job_id: "fixture-job-completed",
          job_type: "current_params",
          artifact_id: "fixture-artifact-available",
          start: "2026-05-17T06:39:00.000Z",
          end: "2026-05-17T06:41:00.000Z",
          balance: 200,
        },
        metrics: {
          net_pnl: 12.5,
          max_drawdown: -2,
          win_rate: 0.5,
          trade_count: 2,
          signal_count: 4,
        },
        diagnostics: [],
      }),
    };
    mockUseReport.mockReturnValue({
      data: report,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchReport>);
    mockUseJson.mockReturnValue({
      isLoading: false,
      isError: false,
      data: {
        schema_version: 2,
        provenance: {
          job_id: "fixture-job-completed",
          job_type: "current_params",
          artifact_id: "fixture-artifact-available",
        },
        metrics: {
          net_pnl: 12.5,
          max_drawdown: -2,
          win_rate: 0.5,
          trade_count: 2,
        },
        equity_curve: [
          { ts: 1, equity: 200 },
          { ts: 2, equity: 212.5 },
        ],
        drawdown_curve: [
          {
            ts: 1,
            equity: 200,
            high_water_mark: 200,
            drawdown: 0,
            drawdown_pct: 0,
          },
          {
            ts: 2,
            equity: 212.5,
            high_water_mark: 212.5,
            drawdown: 0,
            drawdown_pct: 0,
          },
        ],
        rejection_reasons: [{ reason: "window_too_late", count: 3 }],
      },
    } as ReturnType<typeof useResearchReportJson>);

    render(<ResearchReportDetailPage />, { wrapper });

    expect(screen.getByText(/net pnl/i)).toBeInTheDocument();
    expect(screen.getByText(/\+\$12\.50/)).toBeInTheDocument();
    expect(screen.getByText(/equity curve/i)).toBeInTheDocument();
    expect(screen.getByText(/top rejection reasons/i)).toBeInTheDocument();
    expect(screen.getByText(/window_too_late/)).toBeInTheDocument();
  });
});
