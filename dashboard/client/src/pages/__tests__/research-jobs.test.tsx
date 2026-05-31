import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { createElement } from "react";
import { MemoryRouter } from "react-router-dom";

vi.mock("react-router-dom", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router-dom")>()),
  useNavigate: () => vi.fn(),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-jobs", () => ({
  useResearchJobs: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchJobsPage } from "../research-jobs";
import { useResearchJobs } from "../../hooks/use-research-jobs";
import { useAuthStore } from "../../stores/auth-store";

const mockUseJobs = vi.mocked(useResearchJobs);

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return createElement(
    QueryClientProvider,
    { client: qc },
    createElement(MemoryRouter, null, children),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseJobs.mockReturnValue({
    data: { jobs: [] },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchJobs>);
});

describe("ResearchJobsPage - permission gating", () => {
  it("admin sees enabled 'New job' button", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "1", username: "admin", role: "admin" },
    });
    render(<ResearchJobsPage />, { wrapper });
    const btn = screen.getByRole("button", { name: /new job/i });
    expect(btn).not.toBeDisabled();
  });

  it("observer sees disabled 'New job' button with admin hint", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchJobsPage />, { wrapper });
    const btn = screen.getByRole("button", { name: /new job/i });
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute("title", expect.stringMatching(/admin/i));
  });
});

describe("ResearchJobsPage - states", () => {
  it("renders empty state when no jobs", () => {
    render(<ResearchJobsPage />, { wrapper });
    expect(screen.getByText(/no jobs yet/i)).toBeInTheDocument();
  });

  it("renders filtered empty state when jobs do not match selected filters", () => {
    mockUseJobs.mockReturnValue({
      data: {
        jobs: [
          {
            id: "job-1",
            job_type: "current_params",
            artifact_id: "artifact-1",
            status: "completed",
            priority: 1,
            requested_by: "admin",
            params_json: null,
            created_at: 1,
            updated_at: 2,
            cancelled_at: null,
            completed_at: 2,
          },
        ],
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJobs>);
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/research/jobs?q=missing"]}>
          <ResearchJobsPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(
      screen.getByText(/no jobs match the selected filters/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/no jobs yet/i)).not.toBeInTheDocument();
  });

  it("renders Loading on fetch", () => {
    mockUseJobs.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchJobs>);
    render(<ResearchJobsPage />, { wrapper });
    expect(screen.getByTestId("loading")).toBeInTheDocument();
  });

  it("renders danger banner on error", () => {
    mockUseJobs.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("oh no"),
    } as ReturnType<typeof useResearchJobs>);
    render(<ResearchJobsPage />, { wrapper });
    expect(screen.getByText(/could not load jobs/i)).toBeInTheDocument();
  });
});
