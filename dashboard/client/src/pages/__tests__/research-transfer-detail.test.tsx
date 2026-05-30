import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-transfer-running" }),
  useNavigate: () => vi.fn(),
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

import { ResearchTransferDetailPage } from "../research-transfer-detail";
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useResearchTransfer } from "../../hooks/use-research-transfers";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureMachineLive,
  fixtureMachineResearch,
  fixtureTransferRunning,
  fixtureTransferRetryable,
  fixtureTransferCompleted,
} from "../../lib/research-fixtures";

const mockUseTransfer = vi.mocked(useResearchTransfer);
const mockUseMachines = vi.mocked(useResearchMachines);

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
});
