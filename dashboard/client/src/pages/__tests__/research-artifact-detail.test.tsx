import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-artifact-bad-checksum" }),
  useNavigate: () => vi.fn(),
  useLocation: () => ({
    pathname: "/research/artifacts/fixture-artifact-bad-checksum",
    search: "",
    state: null,
  }),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifact: vi.fn(),
  useResearchArtifactManifest: vi.fn(() => ({ isLoading: false, data: null })),
  useResearchArtifactChecksums: vi.fn(() => ({
    isLoading: false,
    data: null,
  })),
}));

vi.mock("../../hooks/use-research-transfers", () => ({
  useResearchTransfers: vi.fn(() => ({ data: { transfers: [] } })),
}));

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachines: vi.fn(() => ({ data: { machines: [] } })),
}));

vi.mock("../../hooks/use-research-jobs", () => ({
  useResearchJobs: vi.fn(() => ({ data: { jobs: [] } })),
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReports: vi.fn(() => ({ data: { reports: [] } })),
}));

vi.mock("../../lib/research-api", () => ({
  archiveResearchArtifact: vi.fn(),
  deleteResearchArtifact: vi.fn(),
  restoreResearchArtifact: vi.fn(),
  updateResearchArtifact: vi.fn(),
  verifyResearchArtifact: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchArtifactDetailPage } from "../research-artifact-detail";
import {
  useResearchArtifact,
  useResearchArtifactChecksums,
  useResearchArtifactManifest,
} from "../../hooks/use-research-artifacts";
import { useResearchMachines } from "../../hooks/use-research-machines";
import { useResearchTransfers } from "../../hooks/use-research-transfers";
import { useResearchJobs } from "../../hooks/use-research-jobs";
import {
  archiveResearchArtifact,
  deleteResearchArtifact,
  restoreResearchArtifact,
  updateResearchArtifact,
  verifyResearchArtifact,
} from "../../lib/research-api";
import { useAuthStore } from "../../stores/auth-store";
import {
  fixtureArtifactArchived,
  fixtureArtifactBadChecksum,
  fixtureMachineLive,
  fixtureMachineResearch,
} from "../../lib/research-fixtures";
import type {
  ArtifactManifest,
  VerifyArtifactResponse,
} from "../../lib/research-types";

const mockUseArtifact = vi.mocked(useResearchArtifact);
const mockUseChecksums = vi.mocked(useResearchArtifactChecksums);
const mockUseManifest = vi.mocked(useResearchArtifactManifest);
const mockUseMachines = vi.mocked(useResearchMachines);
const mockUseTransfers = vi.mocked(useResearchTransfers);
const mockUseJobs = vi.mocked(useResearchJobs);
const mockVerify = vi.mocked(verifyResearchArtifact);
const mockArchive = vi.mocked(archiveResearchArtifact);
const mockRestore = vi.mocked(restoreResearchArtifact);
const mockUpdate = vi.mocked(updateResearchArtifact);
const mockDelete = vi.mocked(deleteResearchArtifact);

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
  mockUseArtifact.mockReturnValue({
    data: fixtureArtifactBadChecksum(),
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchArtifact>);
  mockUseMachines.mockReturnValue({
    data: { machines: [fixtureMachineLive(), fixtureMachineResearch()] },
  } as ReturnType<typeof useResearchMachines>);
  mockUseChecksums.mockReturnValue({
    isLoading: false,
    data: null,
  } as ReturnType<typeof useResearchArtifactChecksums>);
});

describe("ResearchArtifactDetailPage - bad checksum", () => {
  it("surfaces verify failure in a danger banner", async () => {
    mockVerify.mockRejectedValue(new Error("artifact checksum mismatch"));
    render(<ResearchArtifactDetailPage />, { wrapper });

    const verifyBtn = screen.getByRole("button", { name: /^verify$/i });
    await userEvent.click(verifyBtn);

    await waitFor(() =>
      expect(screen.getByText(/verification failed/i)).toBeInTheDocument(),
    );
    expect(
      screen.getByText(/artifact checksum mismatch/i),
    ).toBeInTheDocument();
  });

  it("observer sees verify button disabled with admin hint", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchArtifactDetailPage />, { wrapper });
    const verifyBtn = screen.getByRole("button", { name: /^verify$/i });
    expect(verifyBtn).toBeDisabled();
  });

  it("admin sees delete-with-files button", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /delete with files/i }),
    ).toBeInTheDocument();
  });

  it("requires the artifact id before deleting the metadata record", async () => {
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /delete record/i }));

    const dialog = screen.getByRole("dialog", {
      name: /delete artifact record/i,
    });
    expect(dialog).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", {
      name: "Delete record",
    });
    expect(confirm).toBeDisabled();

    await userEvent.type(
      screen.getByLabelText(/type "fixture-artifact-bad-checksum" to confirm/i),
      "fixture-artifact-bad-checksum",
    );

    expect(confirm).not.toBeDisabled();
  });

  it("renders the manifest 'Load manifest' button gate (no preload)", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /load manifest/i }),
    ).toBeInTheDocument();
  });

  it("uses explicit checksum file labels before and after loading", async () => {
    mockUseChecksums.mockReturnValue({
      isLoading: false,
      data: "abc123  remote-runtime/paint.db",
    } as ReturnType<typeof useResearchArtifactChecksums>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /load checksums\.sha256/i }),
    );

    expect(
      screen.getByRole("button", { name: /hide checksums\.sha256/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/abc123/)).toBeInTheDocument();
  });

  it("renders the 'Labels/notes: unsupported' explicit placeholder", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByText(/labels\/notes: unsupported/i),
    ).toBeInTheDocument();
  });

  it("renders live source machines as provenance labels instead of links", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    const source = screen.getByText("fixture-live");

    expect(source.closest("a")).toBeNull();
  });

  it("resets unsaved metadata edits after cancelling", async () => {
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /edit metadata/i }),
    );
    await screen.findByRole("dialog");
    await userEvent.type(screen.getByLabelText(/source machine id/i), "-draft");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await userEvent.click(
      screen.getByRole("button", { name: /edit metadata/i }),
    );

    expect(screen.getByLabelText(/source machine id/i)).toHaveValue(
      "fixture-live",
    );
  });

  it("humanizes linked transfer and job enums", () => {
    mockUseTransfers.mockReturnValue({
      data: {
        transfers: [
          {
            id: "t-1",
            artifact_id: "fixture-artifact-bad-checksum",
            source_machine_id: "fixture-live",
            dest_machine_id: "fixture-research",
            status: "running",
          },
        ],
      },
    } as ReturnType<typeof useResearchTransfers>);
    mockUseJobs.mockReturnValue({
      data: {
        jobs: [
          {
            id: "j-1",
            artifact_id: "fixture-artifact-bad-checksum",
            job_type: "current_params",
            status: "completed",
          },
        ],
      },
    } as ReturnType<typeof useResearchJobs>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    expect(screen.getAllByText("Running").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Backtest").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Completed").length).toBeGreaterThan(0);
    expect(screen.queryByText("current_params")).not.toBeInTheDocument();
    expect(screen.queryByText("running")).not.toBeInTheDocument();
  });

  it("surfaces verify success counts in a banner", async () => {
    const response: VerifyArtifactResponse = {
      artifact: fixtureArtifactBadChecksum(),
      verification: {
        artifact_id: "fixture-artifact-bad-checksum",
        files_checked: 3,
        bytes_checked: 2048,
      },
    };
    mockVerify.mockResolvedValue(response);
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /^verify$/i }));

    await waitFor(() =>
      expect(
        screen.getByText(/verification succeeded/i),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText(/3 files/i)).toBeInTheDocument();
    expect(mockVerify).toHaveBeenCalledWith("fixture-artifact-bad-checksum");
  });

  it("submits edited metadata fields trimmed and closes the dialog", async () => {
    mockUpdate.mockResolvedValue(fixtureArtifactBadChecksum());
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /edit metadata/i }),
    );
    const dialog = await screen.findByRole("dialog");

    const sourceInput = within(dialog).getByLabelText(/source machine id/i);
    await userEvent.clear(sourceInput);
    await userEvent.type(sourceInput, "  fixture-research  ");
    const runModeInput = within(dialog).getByLabelText(/run mode/i);
    await userEvent.clear(runModeInput);
    await userEvent.type(runModeInput, "paper");

    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: /edit artifact metadata/i }),
      ).not.toBeInTheDocument(),
    );
    expect(mockUpdate).toHaveBeenCalledTimes(1);
    expect(mockUpdate).toHaveBeenCalledWith(
      "fixture-artifact-bad-checksum",
      expect.objectContaining({
        source_machine_id: "fixture-research",
        run_mode: "paper",
      }),
    );
  });

  it("surfaces an update error inside the edit dialog without closing it", async () => {
    mockUpdate.mockRejectedValue(new Error("invalid run mode"));
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /edit metadata/i }),
    );
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(within(dialog).getByText(/could not update/i)).toBeInTheDocument(),
    );
    expect(within(dialog).getByText(/invalid run mode/i)).toBeInTheDocument();
    expect(
      screen.getByRole("dialog", { name: /edit artifact metadata/i }),
    ).toBeInTheDocument();
  });

  it("triggers archive then surfaces a failure banner on rejection", async () => {
    mockArchive.mockRejectedValue(new Error("archive blocked by lock"));
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /^archive$/i }));

    await waitFor(() =>
      expect(screen.getByText(/action failed/i)).toBeInTheDocument(),
    );
    expect(screen.getByText(/archive blocked by lock/i)).toBeInTheDocument();
    expect(mockArchive).toHaveBeenCalledWith("fixture-artifact-bad-checksum");
  });

  it("gates delete-with-files behind the artifact id phrase", async () => {
    mockDelete.mockResolvedValue(fixtureArtifactBadChecksum());
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /delete with files/i }),
    );

    const dialog = screen.getByRole("dialog", {
      name: /delete artifact and files/i,
    });
    const confirm = within(dialog).getByRole("button", {
      name: "Delete artifact and files",
    });
    expect(confirm).toBeDisabled();

    await userEvent.type(
      within(dialog).getByLabelText(
        /type "fixture-artifact-bad-checksum" to confirm/i,
      ),
      "fixture-artifact-bad-checksum",
    );
    expect(confirm).not.toBeDisabled();

    await userEvent.click(confirm);
    await waitFor(() => expect(mockDelete).toHaveBeenCalledWith(
      "fixture-artifact-bad-checksum",
      true,
    ));
  });

  it("renders the manifest file table after loading", async () => {
    const manifest: ArtifactManifest = {
      schema_version: 2,
      artifact_id: "fixture-artifact-bad-checksum",
      kind: "readonly_run",
      source_machine_id: "fixture-live",
      run_mode: "live_readonly",
      created_at_ms: 1_779_000_000_000,
      interval_start_ms: 1_779_000_000_000,
      interval_end_ms: 1_779_000_600_000,
      files: [
        {
          logical_name: "runtime_db",
          kind: "sqlite",
          relative_path: "remote-runtime/paint.db",
          bytes: 4096,
          sha256: "deadbeefcafe0000111122223333444455556666",
        },
      ],
    };
    mockUseManifest.mockReturnValue({
      isLoading: false,
      isError: false,
      data: manifest,
    } as ReturnType<typeof useResearchArtifactManifest>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /load manifest/i }),
    );

    expect(screen.getByText("runtime_db")).toBeInTheDocument();
    expect(screen.getByText("remote-runtime/paint.db")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^hide$/i }),
    ).toBeInTheDocument();
  });

  it("shows a manifest danger banner when the lazy load errors", async () => {
    mockUseManifest.mockReturnValue({
      isLoading: false,
      isError: true,
      error: new Error("manifest fetch failed"),
      data: null,
    } as ReturnType<typeof useResearchArtifactManifest>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(
      screen.getByRole("button", { name: /load manifest/i }),
    );

    expect(screen.getByText(/could not load manifest/i)).toBeInTheDocument();
    expect(screen.getByText(/manifest fetch failed/i)).toBeInTheDocument();
  });

  it("offers prefilled backtest and sweep navigation for admins", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /^backtest$/i }),
    ).not.toBeDisabled();
    expect(
      screen.getByRole("button", { name: /^sweep$/i }),
    ).not.toBeDisabled();
  });

  it("disables the edit-metadata button for observers with an admin hint", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchArtifactDetailPage />, { wrapper });

    const editBtn = screen.getByRole("button", { name: /edit metadata/i });
    expect(editBtn).toBeDisabled();
    expect(editBtn).toHaveAttribute("title", "Admin role required.");
  });

  it("disables backtest and sweep for observers", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchArtifactDetailPage />, { wrapper });

    expect(screen.getByRole("button", { name: /^backtest$/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^sweep$/i })).toBeDisabled();
  });
});

describe("ResearchArtifactDetailPage - archived", () => {
  beforeEach(() => {
    mockUseArtifact.mockReturnValue({
      data: fixtureArtifactArchived(),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchArtifact>);
  });

  it("exposes restore and delete actions but not verify or archive", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });

    expect(
      screen.getByRole("button", { name: /^restore$/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete record/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete with files/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^verify$/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^archive$/i }),
    ).not.toBeInTheDocument();
  });

  it("disables backtest and sweep until the artifact is restored", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });

    const backtest = screen.getByRole("button", { name: /^backtest$/i });
    expect(backtest).toBeDisabled();
    expect(backtest).toHaveAttribute(
      "title",
      "Restore the artifact before starting a backtest.",
    );
  });

  it("invokes restore on click", async () => {
    mockRestore.mockResolvedValue(fixtureArtifactArchived());
    render(<ResearchArtifactDetailPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: /^restore$/i }));

    await waitFor(() =>
      expect(mockRestore).toHaveBeenCalledWith("fixture-artifact-bad-checksum"),
    );
  });
});

describe("ResearchArtifactDetailPage - load states", () => {
  it("renders the loading placeholder while the artifact query is pending", () => {
    mockUseArtifact.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    } as unknown as ReturnType<typeof useResearchArtifact>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    expect(screen.getByTestId("loading")).toBeInTheDocument();
  });

  it("renders a danger banner when the artifact query errors", () => {
    mockUseArtifact.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error("artifact not found"),
    } as unknown as ReturnType<typeof useResearchArtifact>);
    render(<ResearchArtifactDetailPage />, { wrapper });

    expect(screen.getByText(/could not load artifact/i)).toBeInTheDocument();
    expect(screen.getByText(/artifact not found/i)).toBeInTheDocument();
  });
});
