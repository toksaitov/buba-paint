import React from "react";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fixtureMachineDefaultLive,
  fixtureMachineDisabled,
  fixtureMachineSample,
  fixtureMachineTelemetryResponse,
  fixtureMachineTelemetryState,
} from "../../lib/research-fixtures";
import { ResearchMachineDetailPage } from "../research-machine-detail";
import { useResearchMachineTelemetry } from "../../hooks/use-research-machines";

let routeId = "fixture-research";

vi.mock("react-router-dom", () => ({
  Link: ({ children, to }: { children: React.ReactNode; to: string }) => (
    <a href={to}>{children}</a>
  ),
  useParams: () => ({ id: routeId }),
}));

vi.mock("../../hooks/use-research-machines", () => ({
  useResearchMachineTelemetry: vi.fn(),
}));

vi.mock("../../hooks/use-theme", () => ({
  useTheme: () => ({ theme: "dark" }),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading</div>,
}));

vi.mock("recharts", async () => {
  const actual = await vi.importActual<typeof import("recharts")>("recharts");
  return {
    ...actual,
    ResponsiveContainer: ({
      children,
    }: {
      children: React.ReactElement;
    }) => React.cloneElement(children, { width: 600, height: 160 }),
  };
});

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
  unobserve = vi.fn();
}
vi.stubGlobal("ResizeObserver", ResizeObserverMock);

const mockUseTelemetry = vi.mocked(useResearchMachineTelemetry);

beforeEach(() => {
  vi.clearAllMocks();
  routeId = "fixture-research";
  mockUseTelemetry.mockReturnValue({
    data: fixtureMachineTelemetryResponse(),
    isLoading: false,
    isError: false,
    error: null,
  } as ReturnType<typeof useResearchMachineTelemetry>);
});

describe("ResearchMachineDetailPage", () => {
  it("renders healthy host telemetry without machine management or runtime DB controls", () => {
    render(<ResearchMachineDetailPage />);

    for (const heading of ["Identity", "Host", "Worker", "CPU", "Memory & Swap", "Disk"]) {
      expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
    }
    expect(screen.getByText("Telemetry healthy")).toBeInTheDocument();
    expect(screen.getByLabelText("Current CPU usage")).toBeInTheDocument();
    expect(screen.getByLabelText("Current memory usage")).toBeInTheDocument();
    expect(screen.getByLabelText("Current swap usage")).toBeInTheDocument();
    expect(screen.getByLabelText("Current disk usage")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Runtime DB" })).not.toBeInTheDocument();
    for (const label of [/^edit$/i, /^delete$/i, /^disable$/i, /^enable$/i]) {
      expect(screen.queryByRole("button", { name: label })).not.toBeInTheDocument();
    }
  });

  it("handles missing telemetry explicitly", () => {
    mockUseTelemetry.mockReturnValue({
      data: fixtureMachineTelemetryResponse({
        telemetry: null,
        samples: [],
        stale: true,
      }),
      isLoading: false,
      isError: false,
      error: null,
    } as ReturnType<typeof useResearchMachineTelemetry>);

    render(<ResearchMachineDetailPage />);

    expect(screen.getByText("Missing telemetry")).toBeInTheDocument();
    expect(
      screen.getByText(/No typed host telemetry has been recorded/i),
    ).toBeInTheDocument();
  });

  it("surfaces stale, disabled, sampler, worker, and resource warnings", () => {
    const sample = fixtureMachineSample({
      cpu_percent: 96,
      per_core_cpu: [96, 97],
      mem_used_bytes: 15 * 1024 * 1024 * 1024,
      mem_total_bytes: 16 * 1024 * 1024 * 1024,
      mem_available_bytes: 1 * 1024 * 1024 * 1024,
      swap_used_bytes: 2 * 1024 * 1024 * 1024,
      swap_total_bytes: 3 * 1024 * 1024 * 1024,
      disk_used_bytes: 495 * 1024 * 1024 * 1024,
      disk_total_bytes: 500 * 1024 * 1024 * 1024,
    });
    mockUseTelemetry.mockReturnValue({
      data: fixtureMachineTelemetryResponse({
        machine: fixtureMachineDisabled(),
        telemetry: fixtureMachineTelemetryState({
          sampler: {
            sample_interval_ms: 5_000,
            samples_collected: 12,
            last_error: "sampler failed",
          },
          last_error: "worker failed",
        }),
        samples: [sample],
        disabled: true,
        stale: true,
      }),
      isLoading: false,
      isError: false,
      error: null,
    } as ReturnType<typeof useResearchMachineTelemetry>);

    render(<ResearchMachineDetailPage />);

    expect(screen.getByText(/Host needs immediate attention/i)).toBeInTheDocument();
    expect(screen.getByText(/Research worker heartbeat is stale/i)).toBeInTheDocument();
    expect(screen.getByText(/Research host is disabled/i)).toBeInTheDocument();
    expect(screen.getByText(/Sampler error: sampler failed/i)).toBeInTheDocument();
    expect(screen.getByText(/Worker error: worker failed/i)).toBeInTheDocument();
    expect(screen.getByText(/CPU above 90%/i)).toBeInTheDocument();
    expect(screen.getByText(/Available memory below 10%/i)).toBeInTheDocument();
    expect(screen.getByText(/Swap above 50%/i)).toBeInTheDocument();
    expect(screen.getByText(/Free disk before/i)).toBeInTheDocument();
  });

  it("rejects non-research machine detail pages as provenance only", () => {
    routeId = "live";
    mockUseTelemetry.mockReturnValue({
      data: fixtureMachineTelemetryResponse({
        machine: fixtureMachineDefaultLive(),
      }),
      isLoading: false,
      isError: false,
      error: null,
    } as ReturnType<typeof useResearchMachineTelemetry>);

    render(<ResearchMachineDetailPage />);

    expect(screen.getByText("Not a research host")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "CPU" })).not.toBeInTheDocument();
  });
});
