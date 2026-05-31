import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";

const mockNavigate = vi.fn();

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-job-blocked" }),
  useNavigate: () => mockNavigate,
  useLocation: () => ({ pathname: "/research/jobs/fixture-job-blocked", search: "", state: null }),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifacts: vi.fn(),
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

vi.mock("../../lib/research-api", async () => {
  const actual = await vi.importActual<typeof import("../../lib/research-api")>(
    "../../lib/research-api",
  );
  return {
    ...actual,
    appendResearchJobEvent: vi.fn(),
    cloneResearchJob: vi.fn(),
  };
});

import { ResearchJobDetailPage } from "../research-job-detail";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import { useResearchJob } from "../../hooks/use-research-jobs";
import { useResearchReports } from "../../hooks/use-research-reports";
import { useAuthStore } from "../../stores/auth-store";
import { appendResearchJobEvent, cloneResearchJob } from "../../lib/research-api";
import {
  fixtureArtifactAvailable,
  fixtureJobBlocked,
  fixtureJobFailed,
  fixtureJobCompleted,
  fixtureJobRunning,
} from "../../lib/research-fixtures";

const mockUseResearchArtifacts = vi.mocked(useResearchArtifacts);
const mockUseResearchJob = vi.mocked(useResearchJob);
const mockUseResearchReports = vi.mocked(useResearchReports);
const mockCloneResearchJob = vi.mocked(cloneResearchJob);
const mockAppendResearchJobEvent = vi.mocked(appendResearchJobEvent);

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return createElement(QueryClientProvider, { client: qc }, children);
}

beforeEach(() => {
  vi.clearAllMocks();
  mockNavigate.mockReset();
  useAuthStore.setState({
    token: "tok",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockUseResearchArtifacts.mockReturnValue({
    data: { artifacts: [fixtureArtifactAvailable()] },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchArtifacts>);
  mockUseResearchReports.mockReturnValue({
    data: { reports: [] },
  } as ReturnType<typeof useResearchReports>);
  mockAppendResearchJobEvent.mockResolvedValue(undefined);
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
    expect(screen.getByText(/recovery diagnosis/i)).toBeInTheDocument();
    expect(screen.getAllByText(/missing_open_prices/i).length).toBeGreaterThan(
      0,
    );
    expect(
      screen.getByText(/prefer clone with adjusted start and end/i),
    ).toBeInTheDocument();
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
    expect(
      screen.getAllByText(/loaded sweep dimensions/i).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText(/^2$/).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /^retry$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - clone flow", () => {
  it("opens a clone dialog instead of immediately mutating", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /^clone$/i }));

    expect(mockCloneResearchJob).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: /clone job/i });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText(/fixture-job-blocked/i)).toBeInTheDocument();
  });

  it("prefills clone params, preserves unknown params, and submits edited values", async () => {
    const user = userEvent.setup();
    const cloned = fixtureJobCompleted();
    cloned.job.id = "fixture-job-cloned";
    mockCloneResearchJob.mockResolvedValue(cloned);
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^clone$/i }));

    expect(screen.getByLabelText(/source artifact/i)).toHaveValue(
      "fixture-artifact-available",
    );
    expect(screen.getByLabelText(/balance/i)).toHaveValue("200");
    expect(screen.getByLabelText(/set overrides key 1/i)).toHaveValue("RISK");
    expect(screen.getByLabelText(/set overrides value 1/i)).toHaveValue("low");
    expect(
      (screen.getByLabelText(/additional params json/i) as HTMLTextAreaElement)
        .value,
    ).toContain("preserved_unknown");

    fireEvent.change(screen.getByLabelText(/^Start/i), {
      target: { value: "2026-05-17T13:39" },
    });
    fireEvent.change(screen.getByLabelText(/^End/i), {
      target: { value: "2026-05-17T13:41" },
    });
    await user.click(screen.getByRole("button", { name: /create clone/i }));

    await waitFor(() => expect(mockCloneResearchJob).toHaveBeenCalledTimes(1));
    expect(mockCloneResearchJob).toHaveBeenCalledWith(
      "fixture-job-blocked",
      expect.objectContaining({
        artifact_id: "fixture-artifact-available",
        priority: 0,
        params: expect.objectContaining({
          preserved_unknown: "keep me",
          start_ms: Date.parse("2026-05-17T13:39"),
          end_ms: Date.parse("2026-05-17T13:41"),
          balance: 200,
          set: ["RISK=low"],
        }),
      }),
    );
    expect(mockNavigate).toHaveBeenCalledWith("/research/jobs/fixture-job-cloned");
  });

  it("blocks clone submission when additional params JSON is invalid", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^clone$/i }));
    fireEvent.change(screen.getByLabelText(/additional params json/i), {
      target: { value: "{" },
    });

    expect(screen.getByRole("button", { name: /create clone/i })).toBeDisabled();
    expect(
      screen.getAllByText(/expected property name|unexpected end|invalid json/i)
        .length,
    ).toBeGreaterThan(0);
  });

  it("requires confirmation for fallback-derived clone intervals", async () => {
    const user = userEvent.setup();
    const job = fixtureJobBlocked();
    job.job.params_json = JSON.stringify({
      artifact_id: "fixture-artifact-available",
      balance: 200,
    });
    mockUseResearchJob.mockReturnValue({
      data: job,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^clone$/i }));

    const createClone = screen.getByRole("button", { name: /create clone/i });
    expect(createClone).toBeDisabled();
    await user.click(
      screen.getByRole("checkbox", {
        name: /confirm this interval before creating the job/i,
      }),
    );
    expect(createClone).not.toBeDisabled();
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
    const clone = screen.getByRole("button", { name: /^clone$/i });
    expect(clone).not.toBeDisabled();
  });

  it("lets observers inspect clone params but not submit mutations", async () => {
    const user = userEvent.setup();
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
    await user.click(screen.getByRole("button", { name: /^clone$/i }));

    expect(screen.getByRole("dialog", { name: /clone job/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create clone/i })).toBeDisabled();
    expect(
      screen.getByText(/admin role required to create the clone/i),
    ).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - stale leases", () => {
  it("shows clear stale lease guidance for expired running steps", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getByText(/clear stale lease before retrying/i)).toBeInTheDocument();
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

  it("closes the operator note dialog after a note is saved", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /add note/i }));
    const dialog = screen.getByRole("dialog", { name: /add operator note/i });
    await user.type(within(dialog).getByLabelText(/message/i), "QA note");
    await user.click(within(dialog).getByRole("button", { name: /save note/i }));

    await waitFor(() =>
      expect(mockAppendResearchJobEvent).toHaveBeenCalledWith(
        "fixture-job-blocked",
        { level: "info", message: "QA note" },
      ),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: /add operator note/i }),
      ).not.toBeInTheDocument(),
    );
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
