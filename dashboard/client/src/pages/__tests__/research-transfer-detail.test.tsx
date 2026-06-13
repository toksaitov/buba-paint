import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-transfer-running" }),
  useNavigate: () => vi.fn(),
  useLocation: () => ({
    pathname: "/research/transfers/fixture-transfer-running",
    search: "",
    state: null,
  }),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-transfers", () => ({
  useResearchTransfer: vi.fn(),
}));

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachines: vi.fn(() => ({ data: { machines: [] } })),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

vi.mock("../../lib/research-api", () => ({
  pauseArtifactTransfer: vi.fn(() => Promise.resolve({})),
  resumeArtifactTransfer: vi.fn(() => Promise.resolve({})),
  cancelArtifactTransfer: vi.fn(() => Promise.resolve({})),
  retryArtifactTransfer: vi.fn(() => Promise.resolve({})),
  verifyArtifactTransfer: vi.fn(() =>
    Promise.resolve({
      transfer: {},
      verification: {
        artifact_id: "fixture-artifact-available",
        files_checked: 7,
        bytes_checked: 2048,
      },
    }),
  ),
  deleteArtifactTransfer: vi.fn(() => Promise.resolve({})),
}));

import { ResearchTransferDetailPage } from "../research-transfer-detail";
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useResearchTransfer } from "../../hooks/use-research-transfers";
import {
  cancelArtifactTransfer,
  deleteArtifactTransfer,
  pauseArtifactTransfer,
  resumeArtifactTransfer,
  retryArtifactTransfer,
  verifyArtifactTransfer,
} from "../../lib/research-api";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureMachineLive,
  fixtureMachineResearch,
  fixtureTransferRunning,
  fixtureTransferRetryable,
  fixtureTransferPaused,
  fixtureTransferCompleted,
} from "../../lib/research-fixtures";

const mockUseTransfer = vi.mocked(useResearchTransfer);
const mockUseMachines = vi.mocked(useResearchMachines);
const mockPause = vi.mocked(pauseArtifactTransfer);
const mockResume = vi.mocked(resumeArtifactTransfer);
const mockCancel = vi.mocked(cancelArtifactTransfer);
const mockRetry = vi.mocked(retryArtifactTransfer);
const mockVerify = vi.mocked(verifyArtifactTransfer);
const mockDelete = vi.mocked(deleteArtifactTransfer);

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
  mockUseMachines.mockReturnValue({
    data: { machines: [fixtureMachineLive(), fixtureMachineResearch()] },
  } as ReturnType<typeof useResearchMachines>);
});

describe("ResearchTransferDetailPage", () => {
  it("renders partial progress for a running transfer", () => {
    const transfer = fixtureTransferRunning();
    mockUseTransfer.mockReturnValue({
      data: transfer,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });

    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    const ratio = transfer.bytes_done / (transfer.bytes_total ?? 1);
    const pct = (ratio * 100).toFixed(1);
    expect(screen.getByText(new RegExp(`${pct}%`))).toBeInTheDocument();
    expect(
      screen.getByText(/durable state changes immediately/i),
    ).toBeInTheDocument();
  });

  it("renders retry/cancel for retryable transfers", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRetryable(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(screen.getByRole("button", { name: /^retry$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^cancel$/i }),
    ).toBeInTheDocument();
  });

  it("renders verify and delete for completed transfers", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /^verify$/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete record/i }),
    ).toBeInTheDocument();
  });

  it("confirms completed transfer deletion before mutating", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /delete record/i }));

    expect(
      screen.getByRole("heading", { name: "Delete transfer record" }),
    ).toBeInTheDocument();
    const dialog = screen.getByRole("dialog", {
      name: "Delete transfer record",
    });
    expect(
      within(dialog).getByRole("button", { name: /^delete record$/i }),
    ).toBeInTheDocument();
  });

  it("observer sees mutation buttons disabled with admin hint", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    const cancel = screen.getByRole("button", { name: /^cancel$/i });
    expect(cancel).toBeDisabled();
  });

  it("links research destinations but renders live sources as labels", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    const source = screen.getByText("fixture-live");
    const destination = screen.getByText("fixture-research");

    expect(source.closest("a")).toBeNull();
    expect(destination.closest("a")).toHaveAttribute(
      "href",
      "/research/machines/fixture-research",
    );
  });

  it("shows the loading state while the transfer is fetching", () => {
    mockUseTransfer.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(screen.getByTestId("loading")).toBeInTheDocument();
  });

  it("surfaces the query error when the transfer cannot be loaded", () => {
    mockUseTransfer.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("transfer gone"),
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(screen.getByText("Could not load transfer")).toBeInTheDocument();
    expect(screen.getByText("transfer gone")).toBeInTheDocument();
  });

  it("renders the status and checksum chips in the header", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(screen.getAllByText("Completed").length).toBeGreaterThan(0);
    expect(screen.getByText("Checksum: Verified")).toBeInTheDocument();
  });

  it("renders resume and cancel for paused transfers", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferPaused(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /^resume$/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^cancel$/i })).toBeInTheDocument();
  });

  it("renders the error section when the worker reported an error", () => {
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRetryable(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    expect(
      screen.getByText("Worker reported an error"),
    ).toBeInTheDocument();
    expect(screen.getByText("network reset")).toBeInTheDocument();
  });

  it("warns when a running transfer has stalled", () => {
    vi.useFakeTimers();
    const transfer = fixtureTransferRunning();
    vi.setSystemTime(transfer.updated_at + 40 * 60 * 1000);
    mockUseTransfer.mockReturnValue({
      data: transfer,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    try {
      render(<ResearchTransferDetailPage />, { wrapper });
      expect(
        screen.getByText("Transfer may have stalled"),
      ).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("pauses a running transfer through the pause action", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^pause$/i }));

    expect(mockPause).toHaveBeenCalledWith("fixture-transfer-running");
  });

  it("cancels a running transfer through the cancel action", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));

    expect(mockCancel).toHaveBeenCalledWith("fixture-transfer-running");
  });

  it("resumes a paused transfer through the resume action", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferPaused(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^resume$/i }));

    expect(mockResume).toHaveBeenCalledWith("fixture-transfer-running");
  });

  it("retries a retryable transfer with resume requested", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRetryable(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^retry$/i }));

    expect(mockRetry).toHaveBeenCalledWith("fixture-transfer-running", {
      resume: true,
    });
  });

  it("shows a success banner after verifying a completed transfer", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^verify$/i }));

    expect(mockVerify).toHaveBeenCalledWith("fixture-transfer-running");
    expect(
      await screen.findByText("Verification succeeded"),
    ).toBeInTheDocument();
    expect(screen.getByText(/7 files/)).toBeInTheDocument();
  });

  it("shows a failure banner when verification rejects", async () => {
    const user = userEvent.setup();
    mockVerify.mockRejectedValueOnce(new Error("checksum mismatch"));
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^verify$/i }));

    expect(
      await screen.findByText("Verification failed"),
    ).toBeInTheDocument();
    expect(screen.getByText("checksum mismatch")).toBeInTheDocument();
  });

  it("surfaces an action error banner when a mutation rejects", async () => {
    const user = userEvent.setup();
    mockPause.mockRejectedValueOnce(new Error("worker busy"));
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferRunning(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /^pause$/i }));

    expect(await screen.findByText("Action failed")).toBeInTheDocument();
    expect(screen.getByText("worker busy")).toBeInTheDocument();
  });

  it("deletes a completed transfer after confirmation", async () => {
    const user = userEvent.setup();
    mockUseTransfer.mockReturnValue({
      data: fixtureTransferCompleted(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfer>);

    render(<ResearchTransferDetailPage />, { wrapper });
    await user.click(screen.getByRole("button", { name: /delete record/i }));
    const dialog = screen.getByRole("dialog", {
      name: "Delete transfer record",
    });
    await user.click(
      within(dialog).getByRole("button", { name: /^delete record$/i }),
    );

    expect(mockDelete).toHaveBeenCalledWith("fixture-transfer-running");
  });
});
