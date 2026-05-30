import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-job-blocked" }),
  useNavigate: () => vi.fn(),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-jobs", () => ({
  useResearchJob: vi.fn(),
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReports: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchJobDetailPage } from "../research-job-detail";
import { useResearchJob } from "../../hooks/use-research-jobs";
import { useResearchReports } from "../../hooks/use-research-reports";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureJobBlocked,
  fixtureJobFailed,
  fixtureJobCompleted,
} from "../../lib/research-fixtures";

const mockUseResearchJob = vi.mocked(useResearchJob);
const mockUseResearchReports = vi.mocked(useResearchReports);

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
  mockUseResearchReports.mockReturnValue({
    data: { reports: [] },
  } as ReturnType<typeof useResearchReports>);
});

describe("ResearchJobDetailPage - blocked", () => {
  it("renders blocked banner with backend error message and shows valid next actions", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getAllByText(/job is blocked/i).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /^retry$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^continue$/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^cancel$/i }),
    ).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - failed", () => {
  it("renders danger banner with the failed step error", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobFailed(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getAllByText(/job failed/i).length).toBeGreaterThan(0);
    expect(
      screen.getAllByText(/backtest command exited 1/i).length,
    ).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /^retry$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - observer", () => {
  it("renders mutate buttons but disables them with admin hint", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const retry = screen.getByRole("button", { name: /^retry$/i });
    expect(retry).toBeDisabled();
    expect(retry).toHaveAttribute("title", expect.stringMatching(/admin/i));
  });
});

describe("ResearchJobDetailPage - completed", () => {
  it("shows clone, regenerate, and delete (no report references)", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /regenerate report/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete record/i }),
    ).toBeInTheDocument();
  });

  it("shows scratch archive action for completed jobs with reports", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);
    mockUseResearchReports.mockReturnValue({
      data: {
        reports: [
          {
            id: "report-1",
            job_id: "fixture-job-blocked",
            artifact_id: "fixture-artifact-available",
            title: "Completed report",
            status: "available",
            summary_json: null,
            report_path: "/tmp/report.json",
            csv_path: "/tmp/report.csv",
            created_at: 0,
            updated_at: 0,
          },
        ],
      },
    } as ReturnType<typeof useResearchReports>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: /archive scratch dbs/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /delete record/i }),
    ).not.toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - loading and error states", () => {
  it("renders Loading when fetching", () => {
    mockUseResearchJob.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchJob>);
    render(<ResearchJobDetailPage />, { wrapper });
    expect(screen.getByTestId("loading")).toBeInTheDocument();
  });

  it("renders error banner on fetch failure", () => {
    mockUseResearchJob.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("boom"),
    } as ReturnType<typeof useResearchJob>);
    render(<ResearchJobDetailPage />, { wrapper });
    expect(screen.getByText(/could not load job/i)).toBeInTheDocument();
  });
});
