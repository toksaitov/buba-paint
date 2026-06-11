import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueueWaitBanner } from "../queue-wait-banner";
import {
  fixtureJobCompleted,
  fixtureJobRunning,
  fixtureMachineResearch,
  fixtureMachineTelemetryResponse,
  fixtureMachineTelemetryState,
} from "../../../lib/research-fixtures";
import type { ResearchJob } from "../../../lib/research-types";

const machinesMock = vi.fn();
const telemetryMock = vi.fn();

vi.mock("../../../hooks/use-research-machines", () => ({
  useResearchMachines: () => machinesMock(),
  useResearchMachineTelemetry: (id: string, enabled: boolean) =>
    telemetryMock(id, enabled),
}));

function queuedJob(overrides: Partial<ResearchJob> = {}): ResearchJob {
  return {
    ...fixtureJobRunning().job,
    status: "queued",
    created_at: Date.now() - 30_000,
    ...overrides,
  };
}

function mockMachines(): void {
  machinesMock.mockReturnValue({
    data: { machines: [fixtureMachineResearch()] },
  });
}

describe("QueueWaitBanner", () => {
  it("renders nothing for non-queued jobs", () => {
    mockMachines();
    telemetryMock.mockReturnValue({ data: undefined });
    const { container } = render(
      <QueueWaitBanner job={fixtureJobCompleted().job} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows a calm waiting banner for a fresh queued job with live telemetry", () => {
    mockMachines();
    telemetryMock.mockReturnValue({
      data: fixtureMachineTelemetryResponse({
        telemetry: fixtureMachineTelemetryState({
          last_heartbeat_ms: Date.now() - 10_000,
        }),
        stale: false,
      }),
    });
    render(<QueueWaitBanner job={queuedJob()} />);
    expect(screen.getByText(/waiting for a worker/i)).toBeInTheDocument();
    expect(
      screen.getByText(/next idle worker tick should claim/i),
    ).toBeInTheDocument();
  });

  it("escalates when a queued job stays unclaimed past the warning window", () => {
    mockMachines();
    telemetryMock.mockReturnValue({
      data: fixtureMachineTelemetryResponse({
        telemetry: fixtureMachineTelemetryState({
          last_heartbeat_ms: Date.now() - 10_000,
        }),
        stale: false,
      }),
    });
    render(
      <QueueWaitBanner
        job={queuedJob({ created_at: Date.now() - 10 * 60 * 1000 })}
      />,
    );
    expect(
      screen.getByText(/no worker has claimed this job/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/confirm the research worker can reach/i),
    ).toBeInTheDocument();
  });

  it("warns when no worker telemetry is available", () => {
    mockMachines();
    telemetryMock.mockReturnValue({
      data: fixtureMachineTelemetryResponse({ telemetry: null, stale: true }),
    });
    render(<QueueWaitBanner job={queuedJob()} />);
    expect(
      screen.getByText(/no research worker heartbeat has been recorded/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/cannot start/i),
    ).toBeInTheDocument();
  });

  it("requests telemetry only for queued jobs", () => {
    mockMachines();
    telemetryMock.mockReturnValue({ data: undefined });
    render(<QueueWaitBanner job={fixtureJobCompleted().job} />);
    expect(telemetryMock).toHaveBeenCalledWith(expect.any(String), false);
  });
});
