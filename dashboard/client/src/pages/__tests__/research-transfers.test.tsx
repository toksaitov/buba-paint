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
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useResearchTransfers } from "../../hooks/use-research-transfers";
import { useAuthStore } from "../../stores/auth-store";

const mockUseTransfers = vi.mocked(useResearchTransfers);
const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockUseMachines = vi.mocked(useResearchMachines);

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
  useAuthStore.setState({
    token: "token",
    user: { id: "1", username: "admin", role: "admin" },
  });
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
  mockUseArtifacts.mockReturnValue({
    data: {
      artifacts: [
        { id: "artifact-a", status: "available" },
        { id: "artifact-archived", status: "archived" },
      ],
    },
  } as ReturnType<typeof useResearchArtifacts>);
  mockUseMachines.mockReturnValue({
    data: {
      machines: [
        {
          id: "live",
          name: "Live",
          role: "live",
          ssh_alias: null,
          status: "idle",
          details_json: null,
          created_at: 1,
          updated_at: 1,
        },
        {
          id: "research",
          name: "Research",
          role: "research",
          ssh_alias: null,
          status: "idle",
          details_json: null,
          created_at: 1,
          updated_at: 1,
        },
      ],
    },
  } as ReturnType<typeof useResearchMachines>);
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

  it("treats conflicting explicit statuses as custom filters instead of applying the old preset", () => {
    renderTransfers("/research/transfers?preset=attention&status=completed");

    expect(screen.getByLabelText(/transfer preset/i)).toHaveValue("all");
    expect(screen.getByText("completed-transfer")).toBeInTheDocument();
    expect(screen.queryByText("active-transfer")).not.toBeInTheDocument();
  });

  it("manual status changes clear preset-specific filtering", async () => {
    renderTransfers("/research/transfers?preset=attention");

    await userEvent.click(
      screen.getByRole("button", { name: /transfer status filter/i }),
    );
    await userEvent.click(screen.getByRole("button", { name: "clear" }));
    await userEvent.click(screen.getByRole("button", { name: "Completed" }));

    expect(screen.getByLabelText(/transfer preset/i)).toHaveValue("all");
    expect(screen.getByText("completed-transfer")).toBeInTheDocument();
    expect(screen.queryByText("active-transfer")).not.toBeInTheDocument();
  });

  it("blocks non-research transfer destinations before submit", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.selectOptions(
      screen.getByLabelText(/artifact/i),
      "artifact-a",
    );
    await userEvent.selectOptions(
      screen.getByLabelText(/source machine/i),
      "live",
    );
    await userEvent.selectOptions(
      screen.getByLabelText(/destination machine/i),
      "live",
    );

    expect(
      screen.getByText("Destination must be blank or a research host."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Create transfer" }),
    ).toBeDisabled();

    await userEvent.selectOptions(
      screen.getByLabelText(/destination machine/i),
      "research",
    );

    expect(
      screen.queryByText("Destination must be blank or a research host."),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Create transfer" }),
    ).toBeEnabled();
  });

  it("only offers available artifacts in the transfer dialog", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );
    await screen.findByRole("dialog");

    expect(
      screen.getByRole("option", { name: "artifact-a" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "artifact-archived" }),
    ).not.toBeInTheDocument();
  });

  it("explains when no artifacts are transferable", async () => {
    mockUseArtifacts.mockReturnValue({
      data: { artifacts: [{ id: "artifact-archived", status: "archived" }] },
    } as ReturnType<typeof useResearchArtifacts>);

    renderTransfers("/research/transfers?preset=all");

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );
    await screen.findByRole("dialog");

    expect(screen.getByText("No transferable artifacts")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Create transfer" }),
    ).toBeDisabled();
  });

  it("resets unsaved transfer form state after cancelling", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.selectOptions(
      screen.getByLabelText(/artifact/i),
      "artifact-a",
    );
    await userEvent.type(screen.getByLabelText(/bytes total/i), "123");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );

    expect(screen.getByLabelText(/artifact/i)).toHaveValue("");
    expect(screen.getByLabelText(/bytes total/i)).toHaveValue("");
  });

  it("blocks invalid transfer byte totals before submit", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.click(
      screen.getByRole("button", { name: /new transfer/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.selectOptions(
      screen.getByLabelText(/artifact/i),
      "artifact-a",
    );
    await userEvent.type(screen.getByLabelText(/bytes total/i), "0");

    expect(
      screen.getByText("Bytes total must be a positive whole number."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Create transfer" }),
    ).toBeDisabled();

    await userEvent.clear(screen.getByLabelText(/bytes total/i));
    await userEvent.type(screen.getByLabelText(/bytes total/i), "1.5");

    expect(
      screen.getByText("Bytes total must be a positive whole number."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Create transfer" }),
    ).toBeDisabled();
  });
});
