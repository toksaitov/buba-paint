import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { ResearchReportComparePage } from "../research-report-compare";
import type { ResearchReport } from "../../lib/research-types";

vi.mock("../../lib/research-api", () => ({
  getResearchReport: vi.fn(),
  getResearchReportJson: vi.fn(),
}));

import {
  getResearchReport,
  getResearchReportJson,
} from "../../lib/research-api";

const mockGetReport = vi.mocked(getResearchReport);
const mockGetJson = vi.mocked(getResearchReportJson);

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/research/reports/compare?ids=a,b"]}>
        {children}
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function report(id: string): ResearchReport {
  return {
    id,
    job_id: `job-${id}`,
    artifact_id: "artifact-a",
    title: `Report ${id}`,
    status: "available",
    summary_json: null,
    report_path: `/reports/${id}.json`,
    csv_path: `/reports/${id}.csv`,
    created_at: 1,
    updated_at: 2,
  };
}

function payload(
  id: string,
  pnl: number,
  artifact = "artifact-a",
  sourceMismatch = false,
) {
  return {
    schema_version: 2,
    provenance: {
      job_id: `job-${id}`,
      job_type: "current_params",
      artifact_id: artifact,
      start: "2026-05-17T06:39:00.000Z",
      end: "2026-05-17T06:41:00.000Z",
      start_ms: 1,
      end_ms: 2,
      balance: 200,
    },
    metrics: {
      net_pnl: pnl,
      max_drawdown: -1,
      win_rate: 0.5,
      trade_count: 2,
    },
    source_comparison: sourceMismatch
      ? {
          status: "mismatch",
          source: { net_pnl: pnl - 2 },
          replay: { net_pnl: pnl },
          delta: { net_pnl: 2 },
        }
      : null,
    diagnostics: [],
  };
}

beforeEach(() => {
  mockGetReport.mockReset();
  mockGetJson.mockReset();
  mockGetReport.mockImplementation(async (id: string) => report(id));
  mockGetJson.mockImplementation(async (id: string) =>
    id === "a" ? payload(id, 1, "artifact-b", true) : payload(id, 5),
  );
});

describe("ResearchReportComparePage", () => {
  it("ranks reports by Net PnL and warns about incompatible provenance", async () => {
    render(<ResearchReportComparePage />, { wrapper });

    await waitFor(() => {
      expect(
        screen.getByText(/best by replay net pnl: report b/i),
      ).toBeInTheDocument();
    });

    expect(screen.getByText(/different artifacts/i)).toBeInTheDocument();
    expect(screen.getByText(/differ from source-run metrics/i)).toBeInTheDocument();
    expect(screen.getByText("1. Report b")).toBeInTheDocument();
    expect(screen.getByText("+$5.00")).toBeInTheDocument();
    expect(screen.getByText(/source run net pnl/i)).toBeInTheDocument();
    expect(screen.getByText(/replay delta/i)).toBeInTheDocument();
  });

  it("renders the loaded subset and warns when one report fails to load", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    function threeIdWrapper({ children }: { children: ReactNode }) {
      return (
        <QueryClientProvider client={qc}>
          <MemoryRouter
            initialEntries={["/research/reports/compare?ids=a,b,c"]}
          >
            {children}
          </MemoryRouter>
        </QueryClientProvider>
      );
    }
    mockGetReport.mockImplementation(async (id: string) => {
      if (id === "c") throw new Error("not found");
      return report(id);
    });
    mockGetJson.mockImplementation(async (id: string) =>
      payload(id, id === "a" ? 1 : 5),
    );

    render(<ResearchReportComparePage />, { wrapper: threeIdWrapper });

    await waitFor(() => {
      expect(
        screen.getByText(/some reports could not be loaded/i),
      ).toBeInTheDocument();
    });
    expect(screen.getByText(/comparing 2 of 3 reports/i)).toBeInTheDocument();
    expect(screen.getByText(/could not load: c\./i)).toBeInTheDocument();
    expect(screen.getByText("1. Report b")).toBeInTheDocument();
    expect(screen.getByText("2. Report a")).toBeInTheDocument();
    expect(
      screen.queryByText(/could not load comparison/i),
    ).not.toBeInTheDocument();
  });

  it("shows a fatal banner when fewer than two reports load", async () => {
    mockGetReport.mockImplementation(async (id: string) => {
      if (id === "b") throw new Error("archived");
      return report(id);
    });

    render(<ResearchReportComparePage />, { wrapper });

    await waitFor(() => {
      expect(
        screen.getByText(/could not load comparison/i),
      ).toBeInTheDocument();
    });
    expect(screen.getByText(/could not load: b\./i)).toBeInTheDocument();
    expect(screen.queryByText("1. Report a")).not.toBeInTheDocument();
  });
});
