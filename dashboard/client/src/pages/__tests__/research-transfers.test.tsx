import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ArtifactTransfer } from "../../lib/research-types";

vi.mock("../../hooks/use-research-transfers", () => ({
  useResearchTransfers: vi.fn(),
}));

vi.mock("../../lib/research-api", () => ({
  createArtifactTransfer: vi.fn(() => Promise.resolve({ id: "new-transfer" })),
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
import { createArtifactTransfer } from "../../lib/research-api";
import { useAuthStore } from "../../stores/auth-store";

const mockUseTransfers = vi.mocked(useResearchTransfers);
const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockUseMachines = vi.mocked(useResearchMachines);
const mockCreateTransfer = vi.mocked(createArtifactTransfer);

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

  it("shows the loading state while transfers are fetching", () => {
    mockUseTransfers.mockReturnValue({
      data: undefined,
      dataUpdatedAt: 0,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers();

    expect(screen.getByText("Loading transfers")).toBeInTheDocument();
  });

  it("surfaces the query error message when the transfers list fails", () => {
    mockUseTransfers.mockReturnValue({
      data: undefined,
      dataUpdatedAt: 0,
      isLoading: false,
      isError: true,
      error: new Error("backend offline"),
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers();

    expect(screen.getByText("Could not load transfers")).toBeInTheDocument();
    expect(screen.getByText("backend offline")).toBeInTheDocument();
  });

  it("disables the new transfer button for observers", () => {
    useAuthStore.setState({
      token: "token",
      user: { id: "9", username: "obs", role: "observer" },
    });

    renderTransfers();

    expect(
      screen.getByRole("button", { name: /new transfer/i }),
    ).toBeDisabled();
  });

  it("renders progress bytes and checksum chip for each transfer row", () => {
    renderTransfers("/research/transfers?preset=all");

    expect(screen.getByText(/20\.0 B \/ 100 B/)).toBeInTheDocument();
    expect(screen.getByText("Verified")).toBeInTheDocument();
    expect(screen.getAllByRole("progressbar").length).toBeGreaterThan(0);
  });

  it("paused preset filters down to paused transfers", () => {
    mockUseTransfers.mockReturnValue({
      data: {
        transfers: [
          makeTransfer("paused-transfer", "paused"),
          makeTransfer("running-transfer", "running"),
        ],
      },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=paused");

    expect(screen.getByText("paused-transfer")).toBeInTheDocument();
    expect(screen.queryByText("running-transfer")).not.toBeInTheDocument();
  });

  it("cancelled preset filters down to cancelled transfers", () => {
    mockUseTransfers.mockReturnValue({
      data: {
        transfers: [
          makeTransfer("cancelled-transfer", "cancelled"),
          makeTransfer("running-transfer", "running"),
        ],
      },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=cancelled");

    expect(screen.getByText("cancelled-transfer")).toBeInTheDocument();
    expect(screen.queryByText("running-transfer")).not.toBeInTheDocument();
  });

  it("attention preset surfaces retryable and failed transfers", () => {
    mockUseTransfers.mockReturnValue({
      data: {
        transfers: [
          makeTransfer("retryable-transfer", "retryable"),
          makeTransfer("failed-transfer", "failed"),
          makeTransfer("running-transfer", "running"),
        ],
      },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=attention");

    expect(screen.getByText("retryable-transfer")).toBeInTheDocument();
    expect(screen.getByText("failed-transfer")).toBeInTheDocument();
    expect(screen.queryByText("running-transfer")).not.toBeInTheDocument();
  });

  it("checksum_failed preset surfaces transfers with a failed checksum", () => {
    const failedChecksum = makeTransfer("bad-checksum", "completed");
    failedChecksum.checksum_status = "failed";
    mockUseTransfers.mockReturnValue({
      data: {
        transfers: [failedChecksum, makeTransfer("ok-transfer", "completed")],
      },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=checksum_failed");

    expect(screen.getByText("bad-checksum")).toBeInTheDocument();
    expect(screen.queryByText("ok-transfer")).not.toBeInTheDocument();
  });

  it("stale preset surfaces running transfers with no recent progress", () => {
    const stale = makeTransfer("stale-transfer", "running");
    stale.updated_at = 1;
    const fresh = makeTransfer("fresh-transfer", "running");
    fresh.updated_at = 30 * 60 * 1000 + 10;
    mockUseTransfers.mockReturnValue({
      data: { transfers: [stale, fresh] },
      dataUpdatedAt: 30 * 60 * 1000 + 20,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=stale");

    expect(screen.getByText("stale-transfer")).toBeInTheDocument();
    expect(screen.queryByText("fresh-transfer")).not.toBeInTheDocument();
  });

  it("search input narrows the visible transfers", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.type(
      screen.getByLabelText(/search transfers/i),
      "active-transfer",
    );

    expect(screen.getByText("active-transfer")).toBeInTheDocument();
    expect(screen.queryByText("completed-transfer")).not.toBeInTheDocument();
  });

  it("shows an empty state when no transfers match the filters", async () => {
    renderTransfers("/research/transfers?preset=all");

    await userEvent.type(
      screen.getByLabelText(/search transfers/i),
      "no-such-transfer",
    );

    expect(
      screen.getByText("No transfers match the selected filters."),
    ).toBeInTheDocument();
  });

  it("sorting by progress reorders rows by completion ratio", async () => {
    const low = makeTransfer("low-progress", "running");
    low.bytes_done = 10;
    const high = makeTransfer("high-progress", "running");
    high.bytes_done = 90;
    mockUseTransfers.mockReturnValue({
      data: { transfers: [low, high] },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=all");

    await userEvent.selectOptions(
      screen.getByLabelText(/transfer sort/i),
      "progress_desc",
    );

    const links = screen.getAllByRole("link", { name: /-progress$/ });
    expect(links[0]).toHaveTextContent("high-progress");
    expect(links[1]).toHaveTextContent("low-progress");
  });

  it("sorting by created reorders rows by creation time", async () => {
    const older = makeTransfer("older-transfer", "running");
    older.created_at = 1;
    const newer = makeTransfer("newer-transfer", "running");
    newer.created_at = 99;
    mockUseTransfers.mockReturnValue({
      data: { transfers: [older, newer] },
      dataUpdatedAt: 30,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchTransfers>);

    renderTransfers("/research/transfers?preset=all");

    await userEvent.selectOptions(
      screen.getByLabelText(/transfer sort/i),
      "created_desc",
    );

    const links = screen.getAllByRole("link", { name: /-transfer$/ });
    expect(links[0]).toHaveTextContent("newer-transfer");
    expect(links[1]).toHaveTextContent("older-transfer");
  });

  it("submits a valid transfer through the create mutation", async () => {
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
      "research",
    );
    await userEvent.type(screen.getByLabelText(/bytes total/i), "500");
    await userEvent.click(
      screen.getByRole("button", { name: "Create transfer" }),
    );

    expect(mockCreateTransfer).toHaveBeenCalledWith({
      artifact_id: "artifact-a",
      source_machine_id: "live",
      dest_machine_id: "research",
      bytes_total: 500,
    });
  });
});
