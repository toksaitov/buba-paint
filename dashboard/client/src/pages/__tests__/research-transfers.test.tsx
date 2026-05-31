import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ArtifactTransfer } from "../../lib/research-types";

vi.mock("../../hooks/use-research-transfers", () => ({
  useResearchTransfers: vi.fn(),
}));

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifacts: vi.fn(() => ({ data: { artifacts: [] } })),
}));

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachines: vi.fn(() => ({ data: { machines: [] } })),
}));

import { ResearchTransfersPage } from "../research-transfers";
import { useResearchTransfers } from "../../hooks/use-research-transfers";

const mockUseTransfers = vi.mocked(useResearchTransfers);

function makeTransfer(
  id: string,
  status: ArtifactTransfer["status"],
): ArtifactTransfer {
  return {
    id,
    artifact_id: "artifact-a",
    source_machine_id: "live",
    dest_machine_id: "research",
    status,
    bytes_total: 100,
    bytes_done: status === "completed" ? 100 : 20,
    checksum_status: status === "completed" ? "verified" : "pending",
    error: null,
    created_at: 1,
    updated_at: status === "completed" ? 20 : 10,
    completed_at: status === "completed" ? 20 : null,
  };
}

beforeEach(() => {
  mockUseTransfers.mockReturnValue({
    data: {
      transfers: [
        makeTransfer("active-transfer", "running"),
        makeTransfer("completed-transfer", "completed"),
      ],
    },
    dataUpdatedAt: 30,
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchTransfers>);
});

describe("ResearchTransfersPage", () => {
  function renderTransfers(route = "/research/transfers") {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[route]}>
          <ResearchTransfersPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("completed preset updates the status filter so completed rows remain visible", async () => {
    renderTransfers();

    await userEvent.selectOptions(
      screen.getByLabelText(/transfer preset/i),
      "completed",
    );

    expect(screen.getByText("completed-transfer")).toBeInTheDocument();
    expect(screen.queryByText("active-transfer")).not.toBeInTheDocument();
  });

  it("completed preset direct URLs derive completed statuses when status is absent", () => {
    renderTransfers("/research/transfers?preset=completed");

    expect(screen.getByText("completed-transfer")).toBeInTheDocument();
    expect(screen.queryByText("active-transfer")).not.toBeInTheDocument();
  });

  it("all preset direct URLs derive all statuses when status is absent", () => {
    renderTransfers("/research/transfers?preset=all");

    expect(screen.getByText("completed-transfer")).toBeInTheDocument();
    expect(screen.getByText("active-transfer")).toBeInTheDocument();
  });

  it("explicit status parameters still override preset defaults", () => {
    renderTransfers("/research/transfers?preset=all&status=running");

    expect(screen.queryByText("completed-transfer")).not.toBeInTheDocument();
    expect(screen.getByText("active-transfer")).toBeInTheDocument();
  });
});
