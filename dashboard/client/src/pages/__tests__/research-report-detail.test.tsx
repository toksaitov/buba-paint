import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

vi.mock("../../lib/research-api", () => ({
  archiveResearchReport: vi.fn(() => Promise.resolve({})),
  restoreResearchReport: vi.fn(() => Promise.resolve({})),
  deleteResearchReport: vi.fn(() => Promise.resolve({})),
  updateResearchReport: vi.fn(() => Promise.resolve({})),
  downloadResearchReportCsvFromText: vi.fn(),
}));

vi.mock("../../hooks/use-theme", () => ({
  useTheme: () => ({ theme: "light" }),
}));

vi.mock("recharts", () => ({
  CartesianGrid: () => createElement("g", { "data-testid": "grid" }),
  Line: () => createElement("path", { "data-testid": "line" }),
  LineChart: ({ children }: { children: ReactNode }) =>
    createElement("div", { "data-testid": "line-chart" }, children),
  ResponsiveContainer: ({
    children,
    width,
    height,
    minWidth,
  }: {
    children: ReactNode;
    width?: string | number;
    height?: string | number;
    minWidth?: string | number;
  }) =>
    createElement(
      "div",
      {
        "data-testid": "responsive",
        "data-width": width,
        "data-height": height,
        "data-min-width": minWidth,
      },
      children,
    ),
  Tooltip: () => createElement("div", { "data-testid": "tooltip" }),
  XAxis: () => createElement("div", { "data-testid": "x-axis" }),
  YAxis: (props: {
    dataKey?: string;
    domain?: [number, number];
    tickFormatter?: (value: number) => string;
  }) =>
    createElement("div", {
      "data-testid": "y-axis",
      "data-key": props.dataKey ?? "",
      "data-domain": props.domain?.join(",") ?? "",
      "data-sample": props.tickFormatter?.(136.699998) ?? "",
    }),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchReportDetailPage } from "../research-report-detail";
import {
  useResearchReport,
  useResearchReportCsv,
  useResearchReportJson,
} from "../../hooks/use-research-reports";
import {
  archiveResearchReport,
  deleteResearchReport,
  restoreResearchReport,
  updateResearchReport,
} from "../../lib/research-api";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureReportArchived,
  fixtureReportAvailable,
  fixtureReportCsvPayload,
  fixtureReportJsonPayload,
  fixtureReportMissingFile,
} from "../../lib/research-fixtures";

const mockUseReport = vi.mocked(useResearchReport);
const mockUseJson = vi.mocked(useResearchReportJson);
const mockUseCsv = vi.mocked(useResearchReportCsv);
const mockArchive = vi.mocked(archiveResearchReport);
const mockRestore = vi.mocked(restoreResearchReport);
const mockDelete = vi.mocked(deleteResearchReport);
const mockUpdate = vi.mocked(updateResearchReport);

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
  mockUseCsv.mockReturnValue({
    isLoading: false,
    data: null,
    isError: false,
  } as ReturnType<typeof useResearchReportCsv>);
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

  it("requires the report id before deleting the metadata record", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /delete record/i }));

    const dialog = screen.getByRole("dialog", {
      name: /delete report record/i,
    });
    expect(dialog).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", {
      name: "Delete record",
    });
    expect(confirm).toBeDisabled();

    await userEvent.type(
      screen.getByLabelText(/type "fixture-report-missing-file" to confirm/i),
      "fixture-report-missing-file",
    );

    expect(confirm).not.toBeDisabled();
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

  it("uses explicit JSON and CSV toggle labels", async () => {
    mockUseJson.mockReturnValue({
      isLoading: false,
      data: { schema_version: 2, metrics: { net_pnl: 1 } },
      isError: false,
    } as ReturnType<typeof useResearchReportJson>);
    render(<ResearchReportDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: "Show JSON" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Report JSON is hidden.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Load CSV" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /show raw/i }),
    ).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Show JSON" }));

    expect(screen.getByRole("button", { name: "Hide JSON" })).toBeInTheDocument();
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
        source_comparison: {
          status: "mismatch",
          source: {
            net_pnl: 10,
            final_balance: 210,
            trade_count: 2,
            signal_count: 3,
          },
          replay: {
            net_pnl: 12.5,
            final_balance: 212.5,
            trade_count: 2,
            signal_count: 4,
          },
          delta: {
            net_pnl: 2.5,
            final_balance: 2.5,
            trade_count: 0,
            signal_count: 1,
          },
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
        source_comparison: {
          status: "mismatch",
          source: {
            net_pnl: 10,
            final_balance: 210,
            trade_count: 2,
            signal_count: 3,
          },
          replay: {
            net_pnl: 12.5,
            final_balance: 212.5,
            trade_count: 2,
            signal_count: 4,
          },
          delta: {
            net_pnl: 2.5,
            final_balance: 2.5,
            trade_count: 0,
            signal_count: 1,
          },
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

    expect(screen.getAllByText(/replay net pnl/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/source run net pnl/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/replay delta/i)).toBeInTheDocument();
    expect(screen.getAllByText(/\+\$12\.50/).length).toBeGreaterThan(0);
    expect(screen.getByText(/equity curve/i)).toBeInTheDocument();
    expect(screen.getByText(/top rejection reasons/i)).toBeInTheDocument();
    expect(screen.getByText(/window_too_late/)).toBeInTheDocument();
    expect(
      screen.getByText(/backtest differs from source run/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/source run comparison/i)).toBeInTheDocument();
    expect(screen.getAllByText(/\+\$2\.50/).length).toBeGreaterThan(0);
    expect(screen.getByText(/\+1/)).toBeInTheDocument();
    const axes = screen.getAllByTestId("y-axis");
    expect(axes[0]).toHaveAttribute("data-key", "equity");
    expect(axes[0]).toHaveAttribute("data-domain", "198.75,213.75");
    expect(axes[0]).toHaveAttribute("data-sample", "$137");
    expect(axes[1]).toHaveAttribute("data-key", "drawdown");
    expect(axes[1]).toHaveAttribute("data-domain", "-0.1,0");
    const containers = screen.getAllByTestId("responsive");
    expect(containers[0]).toHaveAttribute("data-height", "220");
    expect(containers[0]).toHaveAttribute("data-min-width", "280");
  });
});

describe("ResearchReportDetailPage - admin action bar", () => {
  it("admin archive button invokes the archive mutation", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /^archive$/i }));

    await waitFor(() => {
      expect(mockArchive).toHaveBeenCalledWith("fixture-report-missing-file");
    });
  });

  it("admin can open and confirm the rename form, calling update", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /rename/i }));

    const titleInput = screen.getByLabelText(/title/i);
    expect(titleInput).toHaveValue("Fixture Report Missing File");

    await userEvent.clear(titleInput);
    await userEvent.type(titleInput, "Renamed Report");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith("fixture-report-missing-file", {
        title: "Renamed Report",
      });
    });
  });

  it("disables the rename Save button when the title is blank", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /rename/i }));
    await userEvent.clear(screen.getByLabelText(/title/i));

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("closes the rename form without saving when Cancel is clicked", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /rename/i }));
    expect(screen.getByText(/rename report/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByLabelText(/title/i)).not.toBeInTheDocument();
    expect(mockUpdate).not.toHaveBeenCalled();
  });

  it("gates delete-with-files behind an exact report id phrase", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /delete with files/i }),
    );

    const dialog = screen.getByRole("dialog", {
      name: /delete report and files/i,
    });
    const confirm = within(dialog).getByRole("button", {
      name: "Delete report and files",
    });
    expect(confirm).toBeDisabled();

    await userEvent.type(
      within(dialog).getByLabelText(
        /type "fixture-report-missing-file" to confirm/i,
      ),
      "wrong-id",
    );
    expect(confirm).toBeDisabled();

    await userEvent.clear(
      within(dialog).getByLabelText(
        /type "fixture-report-missing-file" to confirm/i,
      ),
    );
    await userEvent.type(
      within(dialog).getByLabelText(
        /type "fixture-report-missing-file" to confirm/i,
      ),
      "fixture-report-missing-file",
    );
    expect(confirm).not.toBeDisabled();

    await userEvent.click(confirm);
    await waitFor(() => {
      expect(mockDelete).toHaveBeenCalledWith(
        "fixture-report-missing-file",
        true,
      );
    });
  });

  it("fires the delete-record mutation once the phrase matches", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /delete record/i }),
    );
    const dialog = screen.getByRole("dialog", {
      name: /delete report record/i,
    });
    await userEvent.type(
      within(dialog).getByLabelText(
        /type "fixture-report-missing-file" to confirm/i,
      ),
      "fixture-report-missing-file",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete record" }),
    );

    await waitFor(() => {
      expect(mockDelete).toHaveBeenCalledWith(
        "fixture-report-missing-file",
        false,
      );
    });
  });

  it("does not render an Edit button inside the action bar", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.queryByRole("button", { name: "Edit" }),
    ).not.toBeInTheDocument();
  });
});

describe("ResearchReportDetailPage - archived report", () => {
  beforeEach(() => {
    mockUseReport.mockReturnValue({
      data: fixtureReportArchived(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchReport>);
  });

  it("offers Restore in place of Archive for archived reports", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /^restore$/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^archive$/i }),
    ).not.toBeInTheDocument();
  });

  it("admin restore button invokes the restore mutation", async () => {
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /^restore$/i }));

    await waitFor(() => {
      expect(mockRestore).toHaveBeenCalledWith("fixture-report-missing-file");
    });
  });

  it("renders the archived status chip", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(screen.getByText("Archived")).toBeInTheDocument();
  });
});

describe("ResearchReportDetailPage - observer gating", () => {
  beforeEach(() => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
  });

  it("disables every destructive action and hides Rename", () => {
    render(<ResearchReportDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: /^archive$/i }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /delete record/i }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /delete with files/i }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: /rename/i }),
    ).not.toBeInTheDocument();
  });

  it("attaches an admin-required hint to disabled actions", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /^archive$/i }),
    ).toHaveAttribute("title", "Admin role required.");
  });
});

describe("ResearchReportDetailPage - collapsible sections", () => {
  it("loads report JSON into the viewer after Show JSON", async () => {
    mockUseJson.mockReturnValue({
      isLoading: false,
      isError: false,
      data: fixtureReportJsonPayload(),
    } as ReturnType<typeof useResearchReportJson>);
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: "Show JSON" }));

    expect(screen.getByText(/raw payload/i)).toBeInTheDocument();
    expect(
      screen.queryByText("Report JSON is hidden."),
    ).not.toBeInTheDocument();
  });

  it("renders a CSV preview with a download button after Load CSV", async () => {
    mockUseCsv.mockReturnValue({
      isLoading: false,
      isError: false,
      data: fixtureReportCsvPayload(),
    } as ReturnType<typeof useResearchReportCsv>);
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: "Load CSV" }));

    expect(
      screen.getByRole("button", { name: "Download" }),
    ).toBeInTheDocument();
    expect(screen.getByText("net_pnl")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Hide CSV" }),
    ).toBeInTheDocument();
  });

  it("surfaces a CSV error banner when the CSV fetch fails", async () => {
    mockUseCsv.mockReturnValue({
      isLoading: false,
      isError: true,
      error: new Error("csv boom"),
      data: undefined,
    } as ReturnType<typeof useResearchReportCsv>);
    render(<ResearchReportDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: "Load CSV" }));

    expect(screen.getByText(/could not load csv/i)).toBeInTheDocument();
    expect(screen.getByText(/csv boom/i)).toBeInTheDocument();
  });
});

describe("ResearchReportDetailPage - sweep section", () => {
  it("renders a sweep ranking table from the report payload", () => {
    mockUseJson.mockReturnValue({
      isLoading: false,
      isError: false,
      data: {
        schema_version: 2,
        provenance: { job_id: "fixture-job-completed" },
        metrics: { net_pnl: 5 },
        sweep: {
          columns: ["edge_bps", "pnl"],
          parameter_columns: ["edge_bps"],
          metric_columns: ["pnl", "win_rate"],
          ranked_by: "calibrated_pnl",
          rows: [
            {
              rank: 1,
              params: { edge_bps: 2.5 },
              metrics: { pnl: 120.5, win_rate: 0.62 },
            },
            {
              rank: 2,
              params: { edge_bps: 3 },
              metrics: { pnl: 90.25, win_rate: 0.5 },
            },
          ],
          top_rows: [],
        },
      },
    } as ReturnType<typeof useResearchReportJson>);
    render(<ResearchReportDetailPage />, { wrapper });

    expect(screen.getByText(/sweep ranking by/i)).toBeInTheDocument();
    expect(screen.getByText(/sweep uses calibrated ranking/i)).toBeInTheDocument();
    expect(screen.getByText("62.0%")).toBeInTheDocument();
    expect(screen.getByText("2.5")).toBeInTheDocument();
  });
});
