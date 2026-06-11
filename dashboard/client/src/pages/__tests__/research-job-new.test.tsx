import { describe, expect, it, beforeEach, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { createElement } from "react";
import { renderWithProviders } from "../../test/test-utils";

const mockNavigate = vi.fn();

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>(
    "react-router-dom",
  );
  return {
    ...actual,
    useNavigate: () => mockNavigate,
    useSearchParams: () => [new URLSearchParams()],
    Link: ({ children, to }: { children: ReactNode; to: string }) =>
      createElement("a", { href: to }, children),
  };
});

vi.mock("../../hooks/use-research-artifacts", () => ({
  useResearchArtifacts: vi.fn(),
}));

vi.mock("../../hooks/use-research-templates", () => ({
  useResearchJobTemplates: vi.fn(),
}));

vi.mock("../../lib/research-api", async () => {
  const actual = await vi.importActual<typeof import("../../lib/research-api")>(
    "../../lib/research-api",
  );
  return {
    ...actual,
    createResearchJob: vi.fn(),
  };
});

import { ResearchJobNewPage } from "../research-job-new";
import { useResearchArtifacts } from "../../hooks/use-research-artifacts";
import { useResearchJobTemplates } from "../../hooks/use-research-templates";
import { useAuthStore } from "../../stores/auth-store";
import { createResearchJob } from "../../lib/research-api";
import {
  fixtureArtifactAvailable,
  fixtureJobCompleted,
  FIXTURE_INTERVAL_END_MS,
  FIXTURE_INTERVAL_START_MS,
  FIXTURE_TIMESTAMP_MS,
} from "../../lib/research-fixtures";
import type { ResearchJobTemplate } from "../../lib/research-types";

const mockUseArtifacts = vi.mocked(useResearchArtifacts);
const mockUseTemplates = vi.mocked(useResearchJobTemplates);
const mockCreateJob = vi.mocked(createResearchJob);

function templateFixture(): ResearchJobTemplate {
  return {
    id: "template-current",
    name: "Short current params",
    description: null,
    job_type: "current_params",
    artifact_id: "fixture-artifact-available",
    priority: 7,
    params_json: JSON.stringify({
      start_ms: FIXTURE_INTERVAL_START_MS,
      end_ms: FIXTURE_INTERVAL_START_MS + 120_000,
      balance: 250,
    }),
    status: "active",
    created_by: "fixture-user",
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
    last_used_at: null,
    usage_count: 0,
  };
}

function sweepTemplateFixture(): ResearchJobTemplate {
  return {
    id: "template-sweep",
    name: "Short sweep",
    description: null,
    job_type: "sweep",
    artifact_id: "fixture-artifact-available",
    priority: 4,
    params_json: JSON.stringify({
      start_ms: FIXTURE_INTERVAL_START_MS,
      end_ms: FIXTURE_INTERVAL_END_MS,
      balance: 100,
      sweep: ["LATENCY_ARB_MIN_ASK=0.30,0.35"],
      set_overrides: ["RISK=low"],
    }),
    status: "active",
    created_by: "fixture-user",
    created_at: FIXTURE_TIMESTAMP_MS,
    updated_at: FIXTURE_TIMESTAMP_MS,
    last_used_at: null,
    usage_count: 0,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockNavigate.mockReset();
  useAuthStore.setState({
    token: "tok",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockUseArtifacts.mockReturnValue({
    data: { artifacts: [fixtureArtifactAvailable()] },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchArtifacts>);
  mockUseTemplates.mockReturnValue({
    data: { templates: [templateFixture()] },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchJobTemplates>);
  mockCreateJob.mockResolvedValue(fixtureJobCompleted());
});

describe("ResearchJobNewPage", () => {
  it("loads an active template and submits its template_id with edited values", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ResearchJobNewPage />);

    await user.selectOptions(screen.getByLabelText(/^template/i), "template-current");
    const createFromTemplate = screen.getByRole("button", {
      name: /create from template/i,
    });
    await waitFor(() => expect(createFromTemplate).toBeEnabled());
    await user.click(createFromTemplate);

    await waitFor(() =>
      expect(mockCreateJob).toHaveBeenCalledWith(
        expect.objectContaining({
          job_type: "current_params",
          artifact_id: "fixture-artifact-available",
          priority: 7,
          template_id: "template-current",
        }),
      ),
    );
    expect(mockNavigate).toHaveBeenCalledWith(
      "/research/jobs/fixture-job-completed",
    );
  });

  it("loads backend alias params from sweep templates", async () => {
    const user = userEvent.setup();
    mockUseTemplates.mockReturnValue({
      data: { templates: [sweepTemplateFixture()] },
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchJobTemplates>);

    renderWithProviders(<ResearchJobNewPage />);

    await user.selectOptions(screen.getByLabelText(/^template/i), "template-sweep");
    expect(
      screen.getByRole("button", { name: /create from template/i }),
    ).toBeEnabled();

    const createFromTemplate = screen.getByRole("button", {
      name: /create from template/i,
    });
    await waitFor(() => expect(createFromTemplate).toBeEnabled());
    await user.click(createFromTemplate);

    await waitFor(() =>
      expect(mockCreateJob).toHaveBeenCalledWith(
        expect.objectContaining({
          job_type: "sweep",
          template_id: "template-sweep",
          params: expect.objectContaining({
            set: ["RISK=low"],
            sweeps: ["LATENCY_ARB_MIN_ASK=0.30,0.35"],
          }),
        }),
      ),
    );
  });
});
