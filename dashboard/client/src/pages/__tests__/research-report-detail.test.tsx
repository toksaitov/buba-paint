import { describe, expect, it, beforeEach, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useParams: () => ({ id: "fixture-report-missing-file" }),
  useNavigate: () => vi.fn(),
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement("a", { href: to }, children),
}));

vi.mock("../../hooks/use-research-reports", () => ({
  useResearchReport: vi.fn(),
  useResearchReportJson: vi.fn(),
  useResearchReportCsv: vi.fn(() => ({ isLoading: false, data: null })),
}));

vi.mock("../../hooks/use-theme", () => ({
  useTheme: () => ({ theme: "light" }),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

import { ResearchReportDetailPage } from "../research-report-detail";
import {
  useResearchReport,
  useResearchReportJson,
} from "../../hooks/use-research-reports";
import { useAuthStore } from "../../stores/auth-store";
import { fixtureReportMissingFile } from "../../lib/research-fixtures";

const mockUseReport = vi.mocked(useResearchReport);
const mockUseJson = vi.mocked(useResearchReportJson);

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
  mockUseReport.mockReturnValue({
    data: fixtureReportMissingFile(),
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchReport>);
  mockUseJson.mockReturnValue({
    isLoading: false,
    data: undefined,
    isError: false,
  } as ReturnType<typeof useResearchReportJson>);
});

describe("ResearchReportDetailPage - missing file", () => {
  it("links back to source job and artifact for cross-link verification", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("link", { name: /fixture-job-blocked/ }),
    ).toBeInTheDocument();
  });

  it("renders missing-file danger banner after loading JSON when fetch fails", async () => {
    mockUseJson.mockReturnValue({
      isLoading: false,
      isError: true,
      error: new Error("not found"),
      data: undefined,
    } as ReturnType<typeof useResearchReportJson>);
    render(<ResearchReportDetailPage />, { wrapper });
    const btn = screen.getByRole("button", { name: /load json/i });
    await userEvent.click(btn);
    expect(
      screen.getByText(/report files appear to be missing/i),
    ).toBeInTheDocument();
  });

  it("admin sees delete-with-files button", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByRole("button", { name: /delete with files/i }),
    ).toBeInTheDocument();
  });

  it("observer sees mutate buttons disabled", () => {
    useAuthStore.setState({
      token: "tok",
      user: { id: "2", username: "obs", role: "observer" },
    });
    render(<ResearchReportDetailPage />, { wrapper });
    const archive = screen.getByRole("button", { name: /^archive$/i });
    expect(archive).toBeDisabled();
  });

  it("shows 'Notes/tags: unsupported' explicit placeholder", () => {
    render(<ResearchReportDetailPage />, { wrapper });
    expect(
      screen.getByText(/notes\/tags: unsupported/i),
    ).toBeInTheDocument();
  });
});
