import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ResearchArtifact } from "../../lib/research-types";

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifacts: vi.fn(),
}));

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachines: vi.fn(() => ({ data: { machines: [] } })),
}));

vi.mock("../../hooks/use-research-jobs", () => ({
  useResearchJobs: vi.fn(() => ({ data: { jobs: [] } })),
}));

vi.mock("../../hooks/use-research-transfers", () => ({
  useResearchTransfers: vi.fn(() => ({ data: { transfers: [] } })),
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReports: vi.fn(() => ({ data: { reports: [] } })),
}));

import { ResearchArtifactsPage } from "../research-artifacts";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";

const mockUseArtifacts = vi.mocked(useResearchArtifacts);

function makeArtifact(
  id: string,
  status: ResearchArtifact["status"],
): ResearchArtifact {
  return {
    id,
    source_machine_id: "live",
    kind: "live_readonly",
    status,
    run_mode: "live_readonly",
    artifact_root: `/artifacts/${id}`,
    manifest_path: `/artifacts/${id}/manifest.json`,
    bundle_path: null,
    source_db_path: `/artifacts/${id}/paint.db`,
    interval_start_ms: null,
    interval_end_ms: null,
    bytes: 10,
    checksum: null,
    replay_quality_class: "ok",
    backtest_ready_class: "ok",
    live_fidelity_class: "ok",
    created_at: 1,
    updated_at: 2,
    archived_at: status === "archived" ? 2 : null,
  };
}

beforeEach(() => {
  mockUseArtifacts.mockReturnValue({
    data: {
      artifacts: [
        makeArtifact("available-artifact", "available"),
        makeArtifact("archived-artifact", "archived"),
      ],
    },
    dataUpdatedAt: 2,
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchArtifacts>);
});

describe("ResearchArtifactsPage", () => {
  function renderArtifacts(route = "/research/artifacts") {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[route]}>
          <ResearchArtifactsPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("defaults to available artifacts", () => {
    renderArtifacts();

    expect(screen.getByText("available-artifact")).toBeInTheDocument();
    expect(screen.queryByText("archived-artifact")).not.toBeInTheDocument();
  });

  it("archived preset direct URLs derive archived statuses", () => {
    renderArtifacts("/research/artifacts?preset=archived");

    expect(screen.getByText("archived-artifact")).toBeInTheDocument();
    expect(screen.queryByText("available-artifact")).not.toBeInTheDocument();
  });

  it("all preset direct URLs derive all statuses", () => {
    renderArtifacts("/research/artifacts?preset=all");

    expect(screen.getByText("available-artifact")).toBeInTheDocument();
    expect(screen.getByText("archived-artifact")).toBeInTheDocument();
  });

  it("preset changes keep matching rows visible without explicit status state", async () => {
    renderArtifacts();

    await userEvent.selectOptions(
      screen.getByLabelText(/artifact preset/i),
      "archived",
    );

    expect(screen.getByText("archived-artifact")).toBeInTheDocument();
    expect(screen.queryByText("available-artifact")).not.toBeInTheDocument();
  });
});
