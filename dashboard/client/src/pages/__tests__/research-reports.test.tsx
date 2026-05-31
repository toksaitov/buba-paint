import { describe, expect, it, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import type { ResearchReport } from "../../lib/research-types";

const navigate = vi.fn();

vi.mock("react-router-dom", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router-dom")>()),
  useNavigate: () => navigate,
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReports: vi.fn(),
}));

import { ResearchReportsPage } from "../research-reports";
import { useResearchReports } from "../../hooks/use-research-reports";

const mockUseReports = vi.mocked(useResearchReports);

function makeReport(
  id: string,
  jobType: string,
  netPnl: number | null,
  status: "available" | "archived" = "available",
): ResearchReport {
  return {
    id,
    job_id: `job-${id}`,
    artifact_id: `artifact-${id}`,
    title: `Report ${id}`,
    status,
    summary_json: JSON.stringify({
      schema_version: 2,
      provenance: {
        job_id: `job-${id}`,
        job_type: jobType,
        artifact_id: `artifact-${id}`,
      },
      metrics: {
        net_pnl: netPnl,
        max_drawdown: -2,
        win_rate: 0.5,
        trade_count: id === "a" ? 1 : 4,
      },
      diagnostics: [],
    }),
    report_path: `/reports/${id}.json`,
    csv_path: `/reports/${id}.csv`,
    created_at: 1,
    updated_at: id === "a" ? 10 : 20,
  };
}

beforeEach(() => {
  navigate.mockClear();
  mockUseReports.mockReturnValue({
    data: {
      reports: [
        makeReport("a", "current_params", 2),
        makeReport("b", "sweep", 9),
        makeReport("legacy", "current_params", null, "archived"),
      ],
    },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchReports>);
});

describe("ResearchReportsPage", () => {
  function renderReports(route = "/research/reports") {
    return render(
      <MemoryRouter initialEntries={[route]}>
        <ResearchReportsPage />
      </MemoryRouter>,
    );
  }

  it("sorts by Net PnL and renders analysis metrics", () => {
    renderReports();

    const rows = screen.getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Report b");
    expect(screen.getByText("+$9.00")).toBeInTheDocument();
    expect(screen.getAllByText(/DD -\$2\.00/).length).toBeGreaterThan(0);
  });

  it("filters by job type and analysis availability", async () => {
    renderReports();
    await userEvent.selectOptions(
      screen.getByLabelText(/report job type filter/i),
      "sweep",
    );

    expect(screen.getByText("Report b")).toBeInTheDocument();
    expect(screen.queryByText("Report a")).not.toBeInTheDocument();

    await userEvent.selectOptions(
      screen.getByLabelText(/report analysis filter/i),
      "missing",
    );

    expect(
      screen.getByText(/no reports match the selected filters/i),
    ).toBeInTheDocument();
  });

  it("retention archived filter includes archived reports", async () => {
    renderReports();

    await userEvent.selectOptions(
      screen.getByLabelText(/report retention filter/i),
      "archived",
    );

    expect(screen.getByText("Report legacy")).toBeInTheDocument();
    expect(screen.queryByText("Report a")).not.toBeInTheDocument();
  });

  it("retention archived direct URLs derive archived status", () => {
    renderReports("/research/reports?retention=archived");

    expect(screen.getByText("Report legacy")).toBeInTheDocument();
    expect(screen.queryByText("Report a")).not.toBeInTheDocument();
  });

  it("treats conflicting explicit statuses as custom filters instead of applying the old retention filter", () => {
    renderReports("/research/reports?retention=archived&status=available");

    expect(screen.getByLabelText(/report retention filter/i)).toHaveValue(
      "all",
    );
    expect(screen.getByText("Report a")).toBeInTheDocument();
    expect(screen.queryByText("Report legacy")).not.toBeInTheDocument();
  });

  it("manual status changes clear retention-specific filtering", async () => {
    renderReports("/research/reports?retention=archived");

    await userEvent.click(
      screen.getByRole("button", { name: /report status filter/i }),
    );
    await userEvent.click(screen.getByRole("button", { name: "clear" }));
    await userEvent.click(screen.getByRole("button", { name: "Available" }));

    expect(screen.getByLabelText(/report retention filter/i)).toHaveValue(
      "all",
    );
    expect(screen.getByText("Report a")).toBeInTheDocument();
    expect(screen.queryByText("Report legacy")).not.toBeInTheDocument();
  });

  it("navigates to comparison for two selected reports", async () => {
    renderReports();

    await userEvent.click(screen.getByLabelText(/compare report a/i));
    await userEvent.click(screen.getByLabelText(/compare report b/i));
    await userEvent.click(screen.getByRole("button", { name: /compare selected/i }));

    expect(navigate).toHaveBeenCalledWith("/research/reports/compare?ids=a,b");
  });

  it("hydrates comparison selections from the URL", async () => {
    renderReports("/research/reports?selected=a,b");

    expect(screen.getByLabelText(/compare report a/i)).toBeChecked();
    expect(screen.getByLabelText(/compare report b/i)).toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: /compare selected/i }));

    expect(navigate).toHaveBeenCalledWith("/research/reports/compare?ids=a,b");
  });
});
