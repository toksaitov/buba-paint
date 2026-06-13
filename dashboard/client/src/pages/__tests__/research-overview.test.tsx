import { describe, expect, it, beforeEach, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/test-utils";

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifacts: vi.fn(),
}));

vi.mock("../../hooks/use-research-templates", () => ({
  useResearchQueue: vi.fn(),
  useResearchRetention: vi.fn(),
  useResearchJobTemplates: vi.fn(),
}));

vi.mock("../../lib/research-api", async () => {
  const actual = await vi.importActual<typeof import("../../lib/research-api")>(
    "../../lib/research-api",
  );
  return {
    ...actual,
    archiveResearchJobTemplate: vi.fn(),
    archiveResearchRetention: vi.fn(),
    createResearchJobTemplate: vi.fn(),
    deleteResearchJobTemplate: vi.fn(),
    restoreResearchJobTemplate: vi.fn(),
    updateResearchJobTemplate: vi.fn(),
  };
});

import { ResearchOverviewPage } from "../research-overview";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import {
  useResearchJobTemplates,
  useResearchQueue,
  useResearchRetention,
} from "../../hooks/use-research-templates";
import { useAuthStore } from "../../stores/auth-store";
import {
  archiveResearchJobTemplate,
  archiveResearchRetention,
  createResearchJobTemplate,
  deleteResearchJobTemplate,
  restoreResearchJobTemplate,
} from "../../lib/research-api";
import {
  fixtureArtifactAvailable,
  fixtureJobBlocked,
  fixtureJobCompleted,
  fixtureJobRunning,
  fixtureMachineDisabled,
  fixtureReportAvailable,
  fixtureTransferRetryable,
  FIXTURE_TIMESTAMP_MS,
} from "../../lib/research-fixtures";
import type {
  ResearchJobTemplate,
  ResearchQueueResponse,
  ResearchRetentionResponse,
  RetentionArchiveResponse,
} from "../../lib/research-types";

const mockUseQueue = vi.mocked(useResearchQueue);
const mockUseRetention = vi.mocked(useResearchRetention);
const mockUseTemplates = vi.mocked(useResearchJobTemplates);
const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockArchiveTemplate = vi.mocked(archiveResearchJobTemplate);
const mockRestoreTemplate = vi.mocked(restoreResearchJobTemplate);
const mockDeleteTemplate = vi.mocked(deleteResearchJobTemplate);
const mockCreateTemplate = vi.mocked(createResearchJobTemplate);
const mockArchiveRetention = vi.mocked(archiveResearchRetention);

function templateFixture(
  overrides: Partial<ResearchJobTemplate> = {},
): ResearchJobTemplate {
  return {
    id: "template-current",
    name: "Short backtest",
    description: "Known bounded interval",
    job_type: "current_params",
    artifact_id: "fixture-artifact-available",
    priority: 2,
    params_json: JSON.stringify({ start_ms: 1, end_ms: 2, balance: 200 }),
    status: "active",
    created_by: "fixture-user",
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
    last_used_at: null,
    usage_count: 0,
    ...overrides,
  };
}

function queueFixture(): ResearchQueueResponse {
  const running = fixtureJobRunning();
  const blocked = fixtureJobBlocked();
  const retryableTransfer = fixtureTransferRetryable();
  return {
    generated_at_ms: FIXTURE_TIMESTAMP_MS,
    counts: {
      jobs_total: 3,
      jobs_active: 2,
      jobs_waiting: 0,
      jobs_running: 1,
      jobs_retryable: 0,
      jobs_blocked: 1,
      jobs_failed: 0,
      jobs_completed: 1,
      stale_leases: 0,
      transfers_active: 1,
      transfers_attention: 1,
      disabled_hosts: 1,
    },
    jobs: {
      running: [{ job: running.job, step: running.steps[0], stale: false }],
      waiting: [],
      retryable: [],
      blocked: [{ job: blocked.job, step: blocked.steps[3], stale: false }],
      failed: [],
      stale_leases: [],
    },
    transfers: {
      active: [{ transfer: retryableTransfer, stale: false }],
      attention: [{ transfer: retryableTransfer, stale: false }],
      stale: [],
    },
    disabled_hosts: [
      {
        machine: fixtureMachineDisabled(),
        dependencies: {
          artifacts: 1,
          transfers_as_source: 0,
          transfers_as_destination: 0,
          active_transfers: 0,
          jobs_using_source_artifacts: 1,
          reports_using_source_artifacts: 1,
        },
      },
    ],
    recent_reports: [fixtureReportAvailable()],
    retention: {
      jobs: 1,
      reports: 1,
      artifacts: 1,
      scratch_bytes: 12,
      report_bytes: 10,
      artifact_bytes: 32,
    },
  };
}

function retentionFixture(): ResearchRetentionResponse {
  const completed = fixtureJobCompleted();
  return {
    generated_at_ms: FIXTURE_TIMESTAMP_MS,
    jobs: [
      {
        job: completed.job,
        report: fixtureReportAvailable(),
        scratch_bytes: 12,
        eligible: true,
        skipped_reason: null,
      },
    ],
    reports: [
      {
        report: fixtureReportAvailable(),
        bytes: 10,
        eligible: true,
        skipped_reason: null,
      },
    ],
    artifacts: [
      {
        artifact: fixtureArtifactAvailable(),
        bytes: 32,
        active_dependency_count: 0,
        eligible: true,
        skipped_reason: null,
      },
    ],
    totals: {
      jobs: 1,
      reports: 1,
      artifacts: 1,
      scratch_bytes: 12,
      report_bytes: 10,
      artifact_bytes: 32,
    },
  };
}

function archiveResponse(): RetentionArchiveResponse {
  return {
    jobs: [
      {
        id: "fixture-job-completed",
        status: "archived",
        job: fixtureJobCompleted().job,
        report: fixtureReportAvailable(),
        archive: { deleted_paths: ["prepared-backtest.db"], skipped_paths: [] },
        message: null,
      },
    ],
    reports: [],
    artifacts: [],
    totals: {
      jobs: 0,
      reports: 1,
      artifacts: 1,
      scratch_bytes: 0,
      report_bytes: 10,
      artifact_bytes: 32,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useAuthStore.setState({
    token: "tok",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockUseQueue.mockReturnValue({
    data: queueFixture(),
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchQueue>);
  mockUseRetention.mockReturnValue({
    data: retentionFixture(),
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchRetention>);
  mockUseTemplates.mockReturnValue({
    data: {
      templates: [
        templateFixture(),
        templateFixture({
          id: "template-archived",
          name: "Archived sweep",
          job_type: "sweep",
          status: "archived",
        }),
      ],
    },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchJobTemplates>);
  mockUseArtifacts.mockReturnValue({
    data: { artifacts: [fixtureArtifactAvailable()] },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchArtifacts>);
  mockArchiveTemplate.mockResolvedValue(templateFixture());
  mockRestoreTemplate.mockResolvedValue(
    templateFixture({ id: "template-archived", status: "active" }),
  );
  mockDeleteTemplate.mockResolvedValue(templateFixture());
  mockCreateTemplate.mockResolvedValue(templateFixture());
  mockArchiveRetention.mockResolvedValue(archiveResponse());
});

describe("ResearchOverviewPage", () => {
  it("renders queue groups, disabled host impact, templates, and retention totals", () => {
    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText(/queue cockpit/i)).toBeInTheDocument();
    expect(screen.getByText("fixture-job-running")).toBeInTheDocument();
    expect(screen.getByText("fixture-job-blocked")).toBeInTheDocument();
    expect(screen.getByText(/disabled research hosts/i)).toBeInTheDocument();
    expect(screen.getByText("Fixture Disabled Worker")).toBeInTheDocument();
    expect(screen.getByText("Short backtest")).toBeInTheDocument();
    expect(screen.getByText(/scratch dbs/i)).toBeInTheDocument();
    expect(screen.getAllByText("Fixture Report Available").length).toBeGreaterThan(0);
  });

  function metricCard(label: string): HTMLElement {
    const card = screen
      .getByText(label, { selector: "span" })
      .closest("div.border.border-border");
    expect(card).not.toBeNull();
    return card as HTMLElement;
  }

  it("derives metric card counts and reclaimable bytes from the queue counts", () => {
    renderWithProviders(<ResearchOverviewPage />);

    const active = metricCard("Active jobs");
    expect(within(active).getByText("2")).toBeInTheDocument();
    expect(within(active).getByText("0 waiting")).toBeInTheDocument();

    const attention = metricCard("Attention");
    expect(within(attention).getByText("2")).toBeInTheDocument();
    expect(within(attention).getByText("0 stale leases")).toBeInTheDocument();

    const transfers = metricCard("Transfers");
    expect(within(transfers).getByText("1")).toBeInTheDocument();
    expect(within(transfers).getByText("1 attention")).toBeInTheDocument();

    const retention = metricCard("Retention candidates");
    expect(within(retention).getByText("3")).toBeInTheDocument();
    expect(
      within(retention).getByText(/54\.0 B reclaimable/),
    ).toBeInTheDocument();
  });

  it("renders the attention metric in a neutral tone when nothing needs attention", () => {
    const queue = queueFixture();
    mockUseQueue.mockReturnValue({
      data: {
        ...queue,
        counts: {
          ...queue.counts,
          jobs_blocked: 0,
          jobs_failed: 0,
          jobs_retryable: 0,
          stale_leases: 0,
          transfers_attention: 0,
        },
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    const attention = metricCard("Attention");
    const value = within(attention).getByText("0", {
      selector: "div.text-\\[22px\\]",
    });
    expect(value).not.toHaveClass("text-accent-red");
  });

  it("shows the idle queue message and empty transfer message when nothing is in flight", () => {
    const queue = queueFixture();
    mockUseQueue.mockReturnValue({
      data: {
        ...queue,
        jobs: {
          running: [],
          waiting: [],
          retryable: [],
          blocked: [],
          failed: [],
          stale_leases: [],
        },
        transfers: { active: [], attention: [], stale: [] },
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText(/the queue is idle/i)).toBeInTheDocument();
    expect(
      screen.getByText(/no transfers are active or needing attention/i),
    ).toBeInTheDocument();
    expect(screen.queryByText("No jobs in this group.")).not.toBeInTheDocument();
  });

  it("renders per-group empty placeholders for queue groups and transfer groups without items", () => {
    renderWithProviders(<ResearchOverviewPage />);

    expect(
      screen.getAllByText("No jobs in this group.").length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText("No transfers in this group.").length,
    ).toBeGreaterThan(0);
  });

  it("renders the running step label and links queue jobs to their detail route", () => {
    renderWithProviders(<ResearchOverviewPage />);

    const runningLink = screen
      .getByText("fixture-job-running")
      .closest("a");
    expect(runningLink).toHaveAttribute(
      "href",
      "/research/jobs/fixture-job-running",
    );
    expect(screen.getByText(/step 1: verify artifact/i)).toBeInTheDocument();
  });

  it("marks stale queue jobs and transfers with danger chips", () => {
    const running = fixtureJobRunning();
    const blocked = fixtureJobBlocked();
    const staleTransfer = fixtureTransferRetryable();
    const queue = queueFixture();
    mockUseQueue.mockReturnValue({
      data: {
        ...queue,
        jobs: {
          ...queue.jobs,
          running: [{ job: running.job, step: running.steps[0], stale: true }],
          blocked: [{ job: blocked.job, step: blocked.steps[3], stale: false }],
        },
        transfers: {
          active: [],
          attention: [],
          stale: [{ transfer: staleTransfer, stale: true }],
        },
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText("Stale lease")).toBeInTheDocument();
    expect(screen.getAllByText("Stale").length).toBeGreaterThan(0);
  });

  it("links transfer attention rows to their transfer detail route", () => {
    renderWithProviders(<ResearchOverviewPage />);

    const transferLinks = screen
      .getAllByText("fixture-artifact-available")
      .map((node) => node.closest("a"))
      .filter((node): node is HTMLAnchorElement => node != null);
    expect(
      transferLinks.some(
        (link) =>
          link.getAttribute("href") ===
          "/research/transfers/fixture-transfer-retryable",
      ),
    ).toBe(true);
  });

  it("links disabled research hosts to their machine detail route", () => {
    renderWithProviders(<ResearchOverviewPage />);

    const hostLink = screen.getByText("Fixture Disabled Worker").closest("a");
    expect(hostLink).toHaveAttribute(
      "href",
      "/research/machines/fixture-disabled",
    );
    expect(screen.getByText("0 active transfers")).toBeInTheDocument();
  });

  it("hides the disabled hosts section when no hosts are disabled", () => {
    const queue = queueFixture();
    mockUseQueue.mockReturnValue({
      data: { ...queue, disabled_hosts: [] },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(
      screen.queryByText(/disabled research hosts/i),
    ).not.toBeInTheDocument();
  });

  it("links recent reports and shows the empty state when there are none", () => {
    renderWithProviders(<ResearchOverviewPage />);

    const reportLink = screen
      .getAllByText("Fixture Report Available")
      .map((node) => node.closest("a"))
      .find((node) => node != null);
    expect(reportLink).toHaveAttribute(
      "href",
      "/research/reports/fixture-report-available",
    );
  });

  it("shows the recent reports empty state when the queue has none", () => {
    const queue = queueFixture();
    mockUseQueue.mockReturnValue({
      data: { ...queue, recent_reports: [] },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText("No reports yet.")).toBeInTheDocument();
  });

  it("renders the loading state while the queue query is pending", () => {
    mockUseQueue.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText("Loading research queue")).toBeInTheDocument();
    expect(screen.queryByText(/queue cockpit/i)).not.toBeInTheDocument();
  });

  it("renders the error banner when the queue query fails", () => {
    mockUseQueue.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("queue unreachable"),
    } as ReturnType<typeof useResearchQueue>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(
      screen.getByText("Could not load research queue"),
    ).toBeInTheDocument();
    expect(screen.getByText("queue unreachable")).toBeInTheDocument();
  });

  it("renders the templates empty state when no templates exist", () => {
    mockUseTemplates.mockReturnValue({
      data: { templates: [] },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJobTemplates>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText("No job templates yet.")).toBeInTheDocument();
  });

  it("renders the templates loading and error states from the templates query", () => {
    mockUseTemplates.mockReturnValueOnce({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchJobTemplates>);
    const loadingView = renderWithProviders(<ResearchOverviewPage />);
    expect(screen.getByText("Loading templates")).toBeInTheDocument();
    loadingView.unmount();

    mockUseTemplates.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("templates offline"),
    } as ReturnType<typeof useResearchJobTemplates>);
    renderWithProviders(<ResearchOverviewPage />);
    expect(screen.getByText("Could not load templates")).toBeInTheDocument();
    expect(screen.getByText("templates offline")).toBeInTheDocument();
  });

  it("renders the retention loading and error states from the retention query", () => {
    mockUseRetention.mockReturnValueOnce({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchRetention>);
    const loadingView = renderWithProviders(<ResearchOverviewPage />);
    expect(screen.getByText("Loading retention")).toBeInTheDocument();
    loadingView.unmount();

    mockUseRetention.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("retention offline"),
    } as ReturnType<typeof useResearchRetention>);
    renderWithProviders(<ResearchOverviewPage />);
    expect(screen.getByText("Could not load retention")).toBeInTheDocument();
    expect(screen.getByText("retention offline")).toBeInTheDocument();
  });

  it("renders the template usage metadata and last-used time", () => {
    mockUseTemplates.mockReturnValue({
      data: {
        templates: [
          templateFixture({
            usage_count: 3,
            last_used_at: FIXTURE_TIMESTAMP_MS,
          }),
        ],
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJobTemplates>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText(/used 3 times/i)).toBeInTheDocument();
    expect(screen.getByText(/last used/i)).toBeInTheDocument();
  });

  it("disables admin-only template and retention actions for observers", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });

    renderWithProviders(<ResearchOverviewPage />);

    expect(
      screen.getByRole("button", { name: /new template/i }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /archive template short backtest/i }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /archive selected/i }),
    ).toBeDisabled();
  });

  it("keeps the archive selected action disabled until a candidate is chosen", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ResearchOverviewPage />);

    const archiveButton = screen.getByRole("button", {
      name: /archive selected/i,
    });
    expect(archiveButton).toBeDisabled();

    await user.click(screen.getByLabelText(/fixture-job-completed/i));
    expect(archiveButton).toBeEnabled();
  });

  it("renders retention candidate sub-labels for each eligible group", () => {
    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByText("Backtest · 12.0 B")).toBeInTheDocument();
    expect(screen.getByText("32.0 B · 0 active deps")).toBeInTheDocument();
  });

  it("archives selected retention candidates without selecting unrelated metadata", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ResearchOverviewPage />);

    await user.click(screen.getByLabelText(/fixture-job-completed/i));
    await user.click(screen.getByRole("button", { name: /archive selected/i }));
    const dialog = screen.getByRole("dialog");
    await user.click(
      within(dialog).getByRole("button", { name: /archive selected/i }),
    );

    await waitFor(() =>
      expect(mockArchiveRetention).toHaveBeenCalledWith({
        job_ids: ["fixture-job-completed"],
        report_ids: [],
        artifact_ids: [],
      }),
    );
    expect(screen.getByText(/retention archive complete/i)).toBeInTheDocument();
  });

  it("renders only eligible retention rows and keeps the empty state honest for all-ineligible groups", () => {
    const base = retentionFixture();
    mockUseRetention.mockReturnValue({
      data: {
        ...base,
        jobs: [
          base.jobs[0],
          {
            ...base.jobs[0],
            job: { ...base.jobs[0].job, id: "fixture-job-ineligible" },
            eligible: false,
            skipped_reason: "active dependents",
          },
        ],
        reports: [
          {
            ...base.reports[0],
            report: {
              ...base.reports[0].report,
              id: "fixture-report-ineligible",
            },
            eligible: false,
            skipped_reason: "retention window not elapsed",
          },
        ],
      },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchRetention>);

    renderWithProviders(<ResearchOverviewPage />);

    expect(screen.getByLabelText(/fixture-job-completed/i)).toBeInTheDocument();
    expect(
      screen.queryByText("fixture-job-ineligible"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText("fixture-report-ineligible"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("No report archive candidates."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("retention window not elapsed"),
    ).not.toBeInTheDocument();
  });

  it("creates and mutates shared templates from the manager", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ResearchOverviewPage />);

    await user.click(screen.getByRole("button", { name: /new template/i }));
    await user.clear(screen.getByLabelText(/^name/i));
    await user.type(screen.getByLabelText(/^name/i), "Operator smoke");
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: /save template/i,
      }),
    );
    await waitFor(() =>
      expect(mockCreateTemplate).toHaveBeenCalledWith(
        expect.objectContaining({ name: "Operator smoke" }),
      ),
    );

    await user.click(
      screen.getByRole("button", { name: /archive template short backtest/i }),
    );
    await user.click(
      screen.getByRole("button", { name: /restore template archived sweep/i }),
    );
    await user.click(
      screen.getByRole("button", { name: /delete template short backtest/i }),
    );
    expect(mockDeleteTemplate).not.toHaveBeenCalled();
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: /delete template/i,
      }),
    );

    expect(mockArchiveTemplate).toHaveBeenCalledWith("template-current");
    expect(mockRestoreTemplate).toHaveBeenCalledWith("template-archived");
    expect(mockDeleteTemplate).toHaveBeenCalledWith("template-current");
  });
});
