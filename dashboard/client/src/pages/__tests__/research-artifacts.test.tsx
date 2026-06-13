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
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useAuthStore } from "../../stores/auth-store";

const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockUseMachines = vi.mocked(useResearchMachines);

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
  useAuthStore.setState({
    token: "token",
    user: { id: "1", username: "admin", role: "admin" },
  });
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
      ],
    },
  } as ReturnType<typeof useResearchMachines>);
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

  it("treats conflicting explicit statuses as custom filters instead of applying the old preset", () => {
    renderArtifacts("/research/artifacts?preset=archived&status=available");

    expect(screen.getByLabelText(/artifact preset/i)).toHaveValue("all");
    expect(screen.getByText("available-artifact")).toBeInTheDocument();
    expect(screen.queryByText("archived-artifact")).not.toBeInTheDocument();
  });

  it("switching presets replaces the previous preset statuses", async () => {
    renderArtifacts("/research/artifacts?preset=archived");

    await userEvent.selectOptions(
      screen.getByLabelText(/artifact preset/i),
      "available",
    );

    expect(screen.getByLabelText(/artifact preset/i)).toHaveValue("available");
    expect(screen.getByText("available-artifact")).toBeInTheDocument();
    expect(screen.queryByText("archived-artifact")).not.toBeInTheDocument();
  });

  it("resets unsaved local import form state after cancelling", async () => {
    renderArtifacts();

    await userEvent.click(screen.getByRole("button", { name: /import local/i }));
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/artifact root/i),
      "/research/artifacts/draft",
    );
    await userEvent.type(
      screen.getByLabelText(/artifact id override/i),
      "draft-id",
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await userEvent.click(screen.getByRole("button", { name: /import local/i }));

    expect(screen.getByLabelText(/artifact root/i)).toHaveValue("");
    expect(screen.getByLabelText(/artifact id override/i)).toHaveValue("");
  });

  it("resets unsaved remote register form state after cancelling", async () => {
    renderArtifacts();

    await userEvent.click(
      screen.getByRole("button", { name: /register remote/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/remote artifact root/i),
      "/remote/artifact",
    );
    await userEvent.type(
      screen.getByLabelText(/manifest json/i),
      "draft manifest",
    );
    await userEvent.selectOptions(
      screen.getByLabelText(/source machine/i),
      "live",
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await userEvent.click(
      screen.getByRole("button", { name: /register remote/i }),
    );

    expect(screen.getByLabelText(/remote artifact root/i)).toHaveValue("");
    expect(screen.getByLabelText(/manifest json/i)).toHaveValue("");
    expect(screen.getByLabelText(/source machine/i)).toHaveValue("");
  });

  it("presents the register source machine field as optional", async () => {
    renderArtifacts();

    await userEvent.click(
      screen.getByRole("button", { name: /register remote/i }),
    );
    await screen.findByRole("dialog");

    const sourceField = screen.getByLabelText(/source machine/i);
    expect(sourceField).not.toBeRequired();
    expect(screen.getByText(/optional/i)).toBeInTheDocument();
  });
});
