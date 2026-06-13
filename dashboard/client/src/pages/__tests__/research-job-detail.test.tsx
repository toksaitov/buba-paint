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
    createResearchJobTemplate: vi.fn(),
    cancelResearchJob: vi.fn(),
    pauseResearchJob: vi.fn(),
    resumeResearchJob: vi.fn(),
    continueResearchJob: vi.fn(),
    retryResearchJob: vi.fn(),
    deleteResearchJob: vi.fn(),
    regenerateResearchJobReport: vi.fn(),
    retryResearchStep: vi.fn(),
    cancelResearchStep: vi.fn(),
    clearResearchStepLease: vi.fn(),
    resolveResearchStepBlocker: vi.fn(),
  };
});

import { ResearchJobDetailPage } from "../research-job-detail";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import { useResearchJob } from "../../hooks/use-research-jobs";
import { useResearchReports } from "../../hooks/use-research-reports";
import { useAuthStore } from "../../stores/auth-store";
import {
  appendResearchJobEvent,
  cloneResearchJob,
  createResearchJobTemplate,
  cancelResearchJob,
  resumeResearchJob,
  continueResearchJob,
  retryResearchJob,
  deleteResearchJob,
  regenerateResearchJobReport,
  retryResearchStep,
  clearResearchStepLease,
  resolveResearchStepBlocker,
} from "../../lib/research-api";
import {
  fixtureArtifactAvailable,
  fixtureJobBlocked,
  fixtureJobFailed,
  fixtureJobCompleted,
  fixtureJobCancelled,
  fixtureJobRunning,
  fixtureJobPaused,
} from "../../lib/research-fixtures";

const mockUseResearchArtifacts = vi.mocked(useResearchArtifacts);
const mockUseResearchJob = vi.mocked(useResearchJob);
const mockUseResearchReports = vi.mocked(useResearchReports);
const mockCloneResearchJob = vi.mocked(cloneResearchJob);
const mockAppendResearchJobEvent = vi.mocked(appendResearchJobEvent);
const mockCreateTemplate = vi.mocked(createResearchJobTemplate);
const mockCancelJob = vi.mocked(cancelResearchJob);
const mockResumeJob = vi.mocked(resumeResearchJob);
const mockContinueJob = vi.mocked(continueResearchJob);
const mockRetryJob = vi.mocked(retryResearchJob);
const mockDeleteJob = vi.mocked(deleteResearchJob);
const mockRegenerateReport = vi.mocked(regenerateResearchJobReport);
const mockRetryStep = vi.mocked(retryResearchStep);
const mockClearStepLease = vi.mocked(clearResearchStepLease);
const mockResolveStepBlocker = vi.mocked(resolveResearchStepBlocker);
const startDateLabel = /^start(\s*required)?$/i;
const endDateLabel = /^end(\s*required)?$/i;

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

    expect(screen.getByLabelText(/artifact to replay/i)).toHaveValue(
      "fixture-artifact-available",
    );
    expect(screen.getByLabelText(/balance/i)).toHaveValue("200");
    expect(
      screen.getByLabelText(/parameter overrides parameter 1/i),
    ).toHaveValue("RISK");
    expect(screen.getByLabelText(/parameter overrides value 1/i)).toHaveValue(
      "low",
    );
    expect(
      (screen.getByLabelText(/additional params json/i) as HTMLTextAreaElement)
        .value,
    ).toContain("preserved_unknown");

    await user.click(screen.getByRole("radio", { name: /custom range/i }));
    const editedStart = "2026-05-17T12:41";
    const editedEnd = "2026-05-17T12:42";
    fireEvent.change(await screen.findByLabelText(startDateLabel), {
      target: { value: editedStart },
    });
    fireEvent.change(await screen.findByLabelText(endDateLabel), {
      target: { value: editedEnd },
    });
    const createClone = screen.getByRole("button", { name: /create clone/i });
    await waitFor(() => expect(createClone).not.toBeDisabled());
    await user.click(createClone);

    await waitFor(() => expect(mockCloneResearchJob).toHaveBeenCalledTimes(1));
    expect(mockCloneResearchJob).toHaveBeenCalledWith(
      "fixture-job-blocked",
      expect.objectContaining({
        artifact_id: "fixture-artifact-available",
        priority: 0,
        params: expect.objectContaining({
          preserved_unknown: "keep me",
          start_ms: Date.parse(editedStart),
          end_ms: Date.parse(editedEnd),
          balance: 200,
          set: ["RISK=low"],
        }),
      }),
    );
    expect(mockNavigate).toHaveBeenCalledWith("/research/jobs/fixture-job-cloned");
  });

  it("omits additional params JSON when cloned params are all known fields", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^clone$/i }));

    expect(screen.getByRole("dialog", { name: /clone job/i })).toBeInTheDocument();
    expect(screen.queryByLabelText(/additional params json/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/unknown source params remain/i)).not.toBeInTheDocument();
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

  it("allows short full-artifact clone intervals without fallback confirmation", async () => {
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
    expect(createClone).not.toBeDisabled();
    expect(screen.queryByLabelText(startDateLabel)).not.toBeInTheDocument();
    expect(screen.getAllByText(/full artifact/i).length).toBeGreaterThan(0);
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
  it("shows clear stale lease guidance for overdue running steps", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getAllByText(/refresh overdue/i).length).toBeGreaterThan(0);
    expect(
      screen.getByText(/confirm no worker command is still running/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/expired/i)).not.toBeInTheDocument();
  });

  it("shows fresh lease refresh timing without stale-looking relative text", () => {
    const data = fixtureJobRunning();
    const now = Date.now();
    data.steps = data.steps.map((step, index) => ({
      ...step,
      status: index < 2 ? "completed" : index === 2 ? "running" : "queued",
      started_at: index <= 2 ? now - 10_000 : null,
      completed_at: index < 2 ? now - 5_000 : null,
      lease_owner: index === 2 ? "fixture-worker" : null,
      leased_until_ms: index === 2 ? now + 60_000 : null,
    }));
    mockUseResearchJob.mockReturnValue({
      data,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getByText(/refresh due in/i)).toBeInTheDocument();
    expect(screen.queryByText(/refresh overdue/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/until 0s ago/i)).not.toBeInTheDocument();
  });

  it("does not reuse previous step command output for the active running step", () => {
    const data = fixtureJobRunning();
    data.steps[0] = {
      ...data.steps[0],
      status: "completed",
      output_json: JSON.stringify({
        command_output: {
          status_code: 0,
          stdout: "previous replay stdout",
          stderr: "",
        },
      }),
      completed_at: data.steps[0].started_at,
      lease_owner: null,
      leased_until_ms: null,
    };
    data.steps[1] = {
      ...data.steps[1],
      status: "completed",
      output_json: JSON.stringify({ fixture_step: "validate_replay_data" }),
      completed_at: data.steps[0].started_at,
      lease_owner: null,
      leased_until_ms: null,
    };
    data.steps[2] = {
      ...data.steps[2],
      status: "running",
      lease_owner: "fixture-worker",
      leased_until_ms: 1,
      started_at: data.steps[0].started_at,
    };
    data.events = [
      {
        id: "previous-step-output",
        job_id: data.job.id,
        step_id: data.steps[1].id,
        timestamp_ms: 2,
        level: "info",
        message: "local command worker completed step",
        details_json: JSON.stringify({
          command: {
            program: "buba-paint",
            args: ["validate-replay-data"],
            cwd: "/",
          },
          command_output: {
            status_code: 0,
            stdout: "previous replay stdout",
            stderr: "",
            success: true,
            cancelled: false,
          },
        }),
      },
      {
        id: "active-step-started",
        job_id: data.job.id,
        step_id: data.steps[2].id,
        timestamp_ms: 3,
        level: "info",
        message: "research command started",
        details_json: JSON.stringify({
          program: "buba-paint",
          args: ["validate-backtest-input"],
          cwd: "/",
        }),
      },
    ];
    mockUseResearchJob.mockReturnValue({
      data,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getAllByText(/validate-backtest-input/i).length).toBeGreaterThan(
      0,
    );
    expect(screen.getAllByText(/no output recorded/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/previous replay stdout/i)).not.toBeInTheDocument();
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

  it("resets unsaved operator note text after cancelling", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /add note/i }));
    let dialog = screen.getByRole("dialog", { name: /add operator note/i });
    await user.type(within(dialog).getByLabelText(/message/i), "draft note");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await user.click(screen.getByRole("button", { name: /add note/i }));
    dialog = screen.getByRole("dialog", { name: /add operator note/i });

    expect(within(dialog).getByLabelText(/message/i)).toHaveValue("");
  });

  it("resets unsaved save-template edits after cancelling", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(
      screen.getByRole("button", { name: /save as template/i }),
    );
    let dialog = screen.getByRole("dialog", { name: /save job as template/i });
    await user.type(within(dialog).getByLabelText(/name/i), " draft");
    await user.type(within(dialog).getByLabelText(/description/i), "draft");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await user.click(
      screen.getByRole("button", { name: /save as template/i }),
    );
    dialog = screen.getByRole("dialog", { name: /save job as template/i });

    expect(within(dialog).getByLabelText(/name/i)).toHaveValue(
      "Template from fixture-",
    );
    expect(within(dialog).getByLabelText(/description/i)).toHaveValue("");
  });

  it("saves the job as a template with its type and params", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);
    mockCreateTemplate.mockResolvedValue({ id: "tpl-1" } as never);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(
      screen.getByRole("button", { name: /save as template/i }),
    );
    const dialog = screen.getByRole("dialog", {
      name: /save job as template/i,
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Save template" }),
    );

    await waitFor(() =>
      expect(mockCreateTemplate).toHaveBeenCalledWith(
        expect.objectContaining({ job_type: "current_params" }),
      ),
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

describe("ResearchJobDetailPage - running actions", () => {
  it("exposes only cancel and add-note, with no resume or retry", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const actions = within(
      screen.getByRole("heading", { name: /^actions$/i }).closest("section") ??
        document.body,
    );
    expect(actions.getByRole("button", { name: /^cancel$/i })).toBeInTheDocument();
    expect(
      actions.queryByRole("button", { name: /^resume$/i }),
    ).not.toBeInTheDocument();
    expect(
      actions.queryByRole("button", { name: /^retry$/i }),
    ).not.toBeInTheDocument();
    expect(
      actions.queryByRole("button", { name: /^clone$/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /add note/i }),
    ).toBeInTheDocument();
  });

  it("surfaces the cancellation-in-flight banner once the cancel mutation is pending", async () => {
    const user = userEvent.setup();
    mockCancelJob.mockReturnValue(new Promise(() => undefined));
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    const dialog = screen.getByRole("dialog", { name: /^cancel job$/i });
    await user.click(within(dialog).getByRole("button", { name: /cancel job/i }));

    await waitFor(() =>
      expect(mockCancelJob).toHaveBeenCalledWith("fixture-job-blocked"),
    );
    expect(
      await screen.findByText(/cancellation in flight/i),
    ).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - paused actions", () => {
  it("offers resume, cancel, and clone for a paused job", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobPaused(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: /^resume$/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^cancel$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^retry$/i }),
    ).not.toBeInTheDocument();
  });

  it("resumes a paused job and stores the returned detail", async () => {
    const user = userEvent.setup();
    mockResumeJob.mockResolvedValue(fixtureJobRunning());
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobPaused(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^resume$/i }));

    await waitFor(() =>
      expect(mockResumeJob).toHaveBeenCalledWith("fixture-job-blocked"),
    );
  });

  it("surfaces the action-failed banner when resume rejects", async () => {
    const user = userEvent.setup();
    mockResumeJob.mockRejectedValue(new Error("worker offline"));
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobPaused(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^resume$/i }));

    expect(await screen.findByText(/action failed/i)).toBeInTheDocument();
    expect(screen.getByText(/worker offline/i)).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - cancelled actions", () => {
  it("offers continue, clone, and delete for a cancelled job without reports", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCancelled(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: /^continue$/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^clone$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete record/i }),
    ).toBeInTheDocument();
  });

  it("continues a cancelled job through the run-mutation path", async () => {
    const user = userEvent.setup();
    mockContinueJob.mockResolvedValue(fixtureJobRunning());
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCancelled(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^continue$/i }));

    await waitFor(() =>
      expect(mockContinueJob).toHaveBeenCalledWith("fixture-job-blocked"),
    );
  });

  it("explains that the cancelled job produced no report", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCancelled(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(
      screen.getByText(/this job did not complete, so no report was written/i),
    ).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - retryable actions", () => {
  it("offers retry and cancel for a retryable job and retries it", async () => {
    const user = userEvent.setup();
    const data = fixtureJobRunning();
    data.job.status = "retryable";
    mockRetryJob.mockResolvedValue(fixtureJobRunning());
    mockUseResearchJob.mockReturnValue({
      data,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(screen.getByRole("button", { name: /^cancel$/i })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^clone$/i }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^retry$/i }));

    await waitFor(() =>
      expect(mockRetryJob).toHaveBeenCalledWith("fixture-job-blocked"),
    );
  });
});

describe("ResearchJobDetailPage - regenerate report", () => {
  it("regenerates a completed job's report on click", async () => {
    const user = userEvent.setup();
    mockRegenerateReport.mockResolvedValue({} as never);
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(
      screen.getByRole("button", { name: /regenerate report/i }),
    );

    await waitFor(() =>
      expect(mockRegenerateReport).toHaveBeenCalledWith("fixture-job-blocked"),
    );
  });
});

describe("ResearchJobDetailPage - delete confirmation", () => {
  it("keeps the delete disabled until the job id phrase is typed", async () => {
    const user = userEvent.setup();
    mockDeleteJob.mockResolvedValue(fixtureJobCompleted().job);
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /delete record/i }));

    const dialog = screen.getByRole("dialog", { name: /^delete job$/i });
    const confirm = within(dialog).getByRole("button", {
      name: /^delete job$/i,
    });
    expect(confirm).toBeDisabled();

    await user.type(
      within(dialog).getByLabelText(/type .* to confirm/i),
      "fixture-job-completed",
    );
    expect(confirm).not.toBeDisabled();
    await user.click(confirm);

    await waitFor(() =>
      expect(mockDeleteJob).toHaveBeenCalledWith("fixture-job-blocked"),
    );
  });
});

describe("ResearchJobDetailPage - step controls", () => {
  it("retries the blocked step from its expanded control bar", async () => {
    const user = userEvent.setup();
    mockRetryStep.mockResolvedValue(fixtureJobRunning());
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const toggle = screen.getByRole("button", {
      name: /prepare backtest db/i,
    });
    await user.click(toggle);
    const stepRow = toggle.closest("li");
    expect(stepRow).not.toBeNull();
    const row = within(stepRow as HTMLElement);

    expect(
      row.getByRole("button", { name: /resolve blocker/i }),
    ).toBeInTheDocument();
    await user.click(row.getByRole("button", { name: /^retry$/i }));

    await waitFor(() =>
      expect(mockRetryStep).toHaveBeenCalledWith(
        "fixture-job-blocked",
        "fixture-job-blocked-step-3",
      ),
    );
    expect(mockRetryJob).not.toHaveBeenCalled();
  });

  it("resolves a blocked step's blocker", async () => {
    const user = userEvent.setup();
    mockResolveStepBlocker.mockResolvedValue(fixtureJobRunning());
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const toggle = screen.getByRole("button", {
      name: /prepare backtest db/i,
    });
    await user.click(toggle);
    const row = within(toggle.closest("li") as HTMLElement);
    await user.click(row.getByRole("button", { name: /resolve blocker/i }));

    await waitFor(() =>
      expect(mockResolveStepBlocker).toHaveBeenCalledWith(
        "fixture-job-blocked",
        "fixture-job-blocked-step-3",
      ),
    );
  });

  it("offers a clear stale lease control for an expired leased step", async () => {
    const user = userEvent.setup();
    mockClearStepLease.mockResolvedValue(fixtureJobRunning());
    const data = fixtureJobRunning();
    data.steps[0] = {
      ...data.steps[0],
      status: "leased",
      lease_owner: "fixture-worker",
      leased_until_ms: 1,
    };
    mockUseResearchJob.mockReturnValue({
      data,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const toggle = screen.getByRole("button", { name: /verify artifact/i });
    await user.click(toggle);
    const row = within(toggle.closest("li") as HTMLElement);
    await user.click(row.getByRole("button", { name: /clear stale lease/i }));

    await waitFor(() =>
      expect(mockClearStepLease).toHaveBeenCalledWith(
        "fixture-job-blocked",
        "fixture-job-running-step-0",
      ),
    );
  });

  it("always shows the unsupported skip-step placeholder in step controls", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobBlocked(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    const toggle = screen.getByRole("button", {
      name: /prepare backtest db/i,
    });
    await user.click(toggle);
    const row = within(toggle.closest("li") as HTMLElement);
    expect(row.getByText(/skip step: unsupported/i)).toBeInTheDocument();
  });
});

describe("ResearchJobDetailPage - observer step controls", () => {
  it("disables step actions for observers with an admin hint", async () => {
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

    const toggle = screen.getByRole("button", {
      name: /prepare backtest db/i,
    });
    await user.click(toggle);
    const row = within(toggle.closest("li") as HTMLElement);
    const retry = row.getByRole("button", { name: /^retry$/i });
    expect(retry).toBeDisabled();
    expect(retry).toHaveAttribute("title", expect.stringMatching(/admin/i));
    expect(mockRetryStep).not.toHaveBeenCalled();
  });
});

describe("ResearchJobDetailPage - operator note dialog", () => {
  it("submits a warn-level note chosen from the level segment", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /add note/i }));
    const dialog = screen.getByRole("dialog", { name: /add operator note/i });
    await user.click(within(dialog).getByRole("radio", { name: /^warn$/i }));
    await user.type(within(dialog).getByLabelText(/message/i), "watch leases");
    await user.click(within(dialog).getByRole("button", { name: /save note/i }));

    await waitFor(() =>
      expect(mockAppendResearchJobEvent).toHaveBeenCalledWith(
        "fixture-job-blocked",
        { level: "warn", message: "watch leases" },
      ),
    );
  });

  it("disables save note while the message is empty", async () => {
    const user = userEvent.setup();
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: /add note/i }));
    const dialog = screen.getByRole("dialog", { name: /add operator note/i });
    expect(
      within(dialog).getByRole("button", { name: /save note/i }),
    ).toBeDisabled();
  });
});

describe("ResearchJobDetailPage - linked report", () => {
  it("renders a link to the report when one references the job", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);
    mockUseResearchReports.mockReturnValue({
      data: {
        reports: [
          {
            id: "report-77",
            job_id: "fixture-job-blocked",
            artifact_id: "fixture-artifact-available",
            title: "Linked completed report",
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

    const link = screen.getByRole("link", { name: /linked completed report/i });
    expect(link).toHaveAttribute("href", "/research/reports/report-77");
    expect(screen.getByText("report-77")).toBeInTheDocument();
  });

  it("prompts to regenerate when a completed job has no report", () => {
    mockUseResearchJob.mockReturnValue({
      data: fixtureJobCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJob>);

    render(<ResearchJobDetailPage />, { wrapper });

    expect(
      screen.getByText(/no report found for this completed job/i),
    ).toBeInTheDocument();
  });
});
