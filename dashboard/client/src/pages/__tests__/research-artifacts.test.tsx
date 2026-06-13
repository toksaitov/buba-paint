import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
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

vi.mock("../../lib/research-api", async () => {
  const actual = await vi.importActual<typeof import("../../lib/research-api")>(
    "../../lib/research-api",
  );
  return {
    ...actual,
    importResearchArtifact: vi.fn(),
    registerResearchArtifact: vi.fn(),
  };
});

import { ResearchArtifactsPage } from "../research-artifacts";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useAuthStore } from "../../stores/auth-store";
import {
  importResearchArtifact,
  registerResearchArtifact,
} from "../../lib/research-api";
import { fixtureArtifactAvailable } from "../../lib/research-fixtures";
import { formatBytes } from "../../lib/utils";

const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockUseMachines = vi.mocked(useResearchMachines);
const mockImportArtifact = vi.mocked(importResearchArtifact);
const mockRegisterArtifact = vi.mocked(registerResearchArtifact);

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
  mockImportArtifact.mockReset();
  mockRegisterArtifact.mockReset();
  mockImportArtifact.mockResolvedValue({
    artifact: fixtureArtifactAvailable(),
    verification: {} as never,
  });
  mockRegisterArtifact.mockResolvedValue({
    artifact: fixtureArtifactAvailable(),
    manifest_summary: {
      artifact_id: "fixture-artifact-available",
      files: 1,
      bytes: 10,
    },
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

  it("revealing all artifacts via the preset shows archived rows alongside available", async () => {
    renderArtifacts();

    expect(screen.queryByText("archived-artifact")).not.toBeInTheDocument();

    await userEvent.selectOptions(
      screen.getByLabelText(/artifact preset/i),
      "all",
    );

    expect(screen.getByText("available-artifact")).toBeInTheDocument();
    expect(screen.getByText("archived-artifact")).toBeInTheDocument();
  });

  it("renders artifact row links, run mode, and byte formatting", () => {
    renderArtifacts();

    const link = screen.getByRole("link", { name: "available-artifact" });
    expect(link).toHaveAttribute(
      "href",
      "/research/artifacts/available-artifact",
    );
    const row = link.closest("tr") as HTMLElement;
    expect(within(row).getByText("Live readonly")).toBeInTheDocument();
    expect(within(row).getByText(formatBytes(10))).toBeInTheDocument();
  });

  it("admin import dialog submits and closes on success", async () => {
    renderArtifacts();

    await userEvent.click(screen.getByRole("button", { name: /import local/i }));
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/artifact root/i),
      "/research/artifacts/new-artifact",
    );
    await userEvent.type(
      screen.getByLabelText(/artifact id override/i),
      "new-id",
    );
    await userEvent.selectOptions(
      screen.getByLabelText(/source machine id/i),
      "live",
    );
    await userEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(mockImportArtifact).toHaveBeenCalledWith({
      artifact_root: "/research/artifacts/new-artifact",
      artifact_id: "new-id",
      source_machine_id: "live",
    });
  });

  it("admin import dialog surfaces the API error and stays open on failure", async () => {
    mockImportArtifact.mockRejectedValueOnce(new Error("root not found"));
    renderArtifacts();

    await userEvent.click(screen.getByRole("button", { name: /import local/i }));
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/artifact root/i),
      "/research/artifacts/missing",
    );
    await userEvent.click(screen.getByRole("button", { name: "Import" }));

    expect(await screen.findByText("root not found")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("admin register dialog parses manifest JSON, submits, and closes on success", async () => {
    renderArtifacts();

    await userEvent.click(
      screen.getByRole("button", { name: /register remote/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/remote artifact root/i),
      "/remote/host/artifact",
    );
    await userEvent.type(
      screen.getByLabelText(/manifest json/i),
      '{{"schema_version":1}',
    );
    await userEvent.click(screen.getByRole("button", { name: "Register" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(mockRegisterArtifact).toHaveBeenCalledWith({
      artifact_root: "/remote/host/artifact",
      manifest: { schema_version: 1 },
      source_machine_id: undefined,
    });
  });

  it("admin register dialog rejects invalid manifest JSON without calling the API", async () => {
    renderArtifacts();

    await userEvent.click(
      screen.getByRole("button", { name: /register remote/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.type(
      screen.getByLabelText(/remote artifact root/i),
      "/remote/host/artifact",
    );
    await userEvent.type(
      screen.getByLabelText(/manifest json/i),
      "not json",
    );
    await userEvent.click(screen.getByRole("button", { name: "Register" }));

    expect(
      await screen.findByText(/manifest json is invalid/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(mockRegisterArtifact).not.toHaveBeenCalled();
  });

  it("observers see the create buttons disabled with an admin hint", () => {
    useAuthStore.setState({
      token: "token",
      user: { id: "2", username: "observer", role: "observer" },
    });
    renderArtifacts();

    const importButton = screen.getByRole("button", { name: /import local/i });
    const registerButton = screen.getByRole("button", {
      name: /register remote/i,
    });
    expect(importButton).toBeDisabled();
    expect(registerButton).toBeDisabled();
    expect(importButton).toHaveAttribute("title", "Admin role required.");
    expect(registerButton).toHaveAttribute("title", "Admin role required.");
  });

  it("renders the empty state when no artifacts match", () => {
    mockUseArtifacts.mockReturnValue({
      data: { artifacts: [] },
      dataUpdatedAt: 2,
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchArtifacts>);
    renderArtifacts();

    expect(
      screen.getByText(/no artifacts match the selected filters/i),
    ).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("renders the loading state while artifacts are fetching", () => {
    mockUseArtifacts.mockReturnValue({
      data: undefined,
      dataUpdatedAt: 0,
      isLoading: true,
      isError: false,
    } as ReturnType<typeof useResearchArtifacts>);
    renderArtifacts();

    expect(screen.getByText("Loading artifacts")).toBeInTheDocument();
  });

  it("renders the error banner when the artifacts query fails", () => {
    mockUseArtifacts.mockReturnValue({
      data: undefined,
      dataUpdatedAt: 0,
      isLoading: false,
      isError: true,
      error: new Error("agent unreachable"),
    } as ReturnType<typeof useResearchArtifacts>);
    renderArtifacts();

    expect(screen.getByText("Could not load artifacts")).toBeInTheDocument();
    expect(screen.getByText("agent unreachable")).toBeInTheDocument();
  });
});
