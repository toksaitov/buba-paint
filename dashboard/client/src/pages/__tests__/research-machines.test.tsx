import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fixtureMachineDefaultLive,
  fixtureMachineDisabled,
  fixtureMachineResearch,
  fixtureMachineTelemetryResponse,
} from "../../lib/research-fixtures";
import { ResearchMachinesPage } from "../research-machines";
import {
  useResearchMachines,
  useResearchMachineTelemetry,
} from "../../hooks/use-research-machines";

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachines: vi.fn(),
  useResearchMachineTelemetry: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

const mockUseMachines = vi.mocked(useResearchMachines);
const mockUseTelemetry = vi.mocked(useResearchMachineTelemetry);

function renderPage() {
  render(
    <MemoryRouter>
      <ResearchMachinesPage />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseMachines.mockReturnValue({
    data: {
      machines: [
        fixtureMachineDefaultLive(),
        fixtureMachineResearch(),
        fixtureMachineDisabled(),
      ],
    },
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useResearchMachines>);
  mockUseTelemetry.mockImplementation((id: string) => {
    if (id === "fixture-disabled") {
      return {
        data: fixtureMachineTelemetryResponse({
          machine: fixtureMachineDisabled(),
          disabled: true,
        }),
        isLoading: false,
        isError: false,
      } as ReturnType<typeof useResearchMachineTelemetry>;
    }
    return {
      data: fixtureMachineTelemetryResponse({ machine: fixtureMachineResearch() }),
      isLoading: false,
      isError: false,
    } as ReturnType<typeof useResearchMachineTelemetry>;
  });
});

describe("ResearchMachinesPage", () => {
  it("renders research hosts and hides live provenance machines", () => {
    renderPage();

    expect(screen.getByText("fixture-research")).toBeInTheDocument();
    expect(screen.getByText("fixture-disabled")).toBeInTheDocument();
    expect(screen.queryByText("live")).not.toBeInTheDocument();
    expect(screen.queryByText("Buba Paint Live")).not.toBeInTheDocument();
  });

  it("does not render machine management controls", () => {
    renderPage();

    for (const label of [/new machine/i, /^edit$/i, /^delete$/i, /^disable$/i, /^enable$/i]) {
      expect(screen.queryByRole("button", { name: label })).not.toBeInTheDocument();
    }
  });

  it("renders telemetry state, worker state, and dependency count", () => {
    renderPage();

    expect(screen.getByText("Healthy")).toBeInTheDocument();
    expect(screen.getAllByText("Disabled").length).toBeGreaterThan(0);
    expect(screen.getAllByText(/fixture-worker/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText("8").length).toBeGreaterThan(0);
  });

  it("throttles per-row telemetry polling to a slower cadence", () => {
    renderPage();

    expect(mockUseTelemetry).toHaveBeenCalled();
    for (const call of mockUseTelemetry.mock.calls) {
      expect(call[2]).toBe(20_000);
    }
  });
});
