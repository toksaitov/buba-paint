import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-artifact-bad-checksum" }),
  useNavigate: () => vi.fn(),
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
import { useResearchArtifact } from "../../hooks/use-research-artifacts";
import { verifyResearchArtifact } from "../../lib/research-api";
import { useAuthStore } from "../../stores/auth-store";
import { fixtureArtifactBadChecksum } from "../../lib/research-fixtures";

const mockUseArtifact = vi.mocked(useResearchArtifact);
const mockVerify = vi.mocked(verifyResearchArtifact);

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

  it("renders the manifest 'Load manifest' button gate (no preload)", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /load manifest/i }),
    ).toBeInTheDocument();
  });

  it("renders the 'Labels/notes: unsupported' explicit placeholder", () => {
    render(<ResearchArtifactDetailPage />, { wrapper });
    expect(
      screen.getByText(/labels\/notes: unsupported/i),
    ).toBeInTheDocument();
  });
});
