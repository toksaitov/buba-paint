import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";
import React from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-machine", () => ({
  useMachine: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
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

import { useMachine } from "../../hooks/use-machine";
import { MachinePage } from "../machine";
import type { MachineResponse, MachineSample } from "../../lib/types";

const mockUseMachine = vi.mocked(useMachine);

function baseSample(overrides: Partial<MachineSample> = {}): MachineSample {
  return {
    sampled_at_ms: 1_700_000_000_000,
    cpu_percent: 25,
    per_core_cpu: [22, 28],
    load_one: 0.7,
    load_five: 0.6,
    load_fifteen: 0.5,
    mem_used_bytes: 4 * 1024 * 1024 * 1024,
    mem_total_bytes: 16 * 1024 * 1024 * 1024,
    mem_available_bytes: 12 * 1024 * 1024 * 1024,
    swap_used_bytes: 0,
    swap_total_bytes: 2 * 1024 * 1024 * 1024,
    disk_used_bytes: 100 * 1024 * 1024 * 1024,
    disk_total_bytes: 500 * 1024 * 1024 * 1024,
    disk_mount: "/",
    ...overrides,
  };
}

function baseResponse(
  overrides: Partial<MachineResponse> = {},
): MachineResponse {
  const current = baseSample();
  return {
    host: {
      hostname: "buba-paint",
      os_name: "linux",
      os_version: "6.1",
      kernel_version: "6.1.0",
      cpu_count: 2,
      total_ram_bytes: 16 * 1024 * 1024 * 1024,
    },
    agent_started_at_ms: 1_700_000_000_000,
    current,
    history: [current],
    runtime_db: {
      db_path: "/runtime/paint.db",
      db_bytes: 50_000_000,
      wal_bytes: 1_000_000,
      shm_bytes: 32_768,
    },
    sampler: {
      sample_interval_ms: 5_000,
      samples_collected: 12,
      last_error: null,
    },
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseMachine.mockReturnValue({
    isLoading: true,
    isError: false,
    data: undefined,
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("shows error banner when query errors", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: true,
    data: undefined,
    error: new Error("network down"),
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(
    screen.getByText(/Unable to load machine status/i),
  ).toBeInTheDocument();
});

test("shows sampler empty state when current sample is null", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse({ current: null, history: [] }),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(
    screen.getByText(/Sampler has not produced a snapshot yet/i),
  ).toBeInTheDocument();
});

test("renders all five card headings when data is present", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse(),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  for (const heading of [
    "Host",
    "CPU",
    "Memory & Swap",
    "Disk",
    "Runtime DB",
  ]) {
    expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
  }
});

test("renders donut gauges for cpu, memory, swap, and disk", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse(),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(screen.getByLabelText("Current CPU usage")).toBeInTheDocument();
  expect(screen.getByLabelText("Current memory usage")).toBeInTheDocument();
  expect(screen.getByLabelText("Current swap usage")).toBeInTheDocument();
  expect(screen.getByLabelText("Current disk usage")).toBeInTheDocument();
});

test("renders timeline charts for cpu, memory, disk, and runtime DB", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse(),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(
    screen.getByLabelText(/CPU history, last 5 minutes/),
  ).toBeInTheDocument();
  expect(
    screen.getByLabelText(/Memory and swap history/),
  ).toBeInTheDocument();
  expect(screen.getByLabelText(/Disk usage history/)).toBeInTheDocument();
  expect(screen.getByLabelText(/DB size history/)).toBeInTheDocument();
});

test("surfaces a warning banner when disk usage crosses the warning threshold", () => {
  const current = baseSample({
    disk_used_bytes: 415 * 1024 * 1024 * 1024,
    disk_total_bytes: 500 * 1024 * 1024 * 1024,
  });
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse({ current, history: [current] }),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(screen.getByText(/Host warnings active/i)).toBeInTheDocument();
  expect(screen.getByText(/Plan disk cleanup soon/i)).toBeInTheDocument();
});

test("surfaces a danger banner when ram available is critically low", () => {
  const current = baseSample({
    mem_used_bytes: 15 * 1024 * 1024 * 1024,
    mem_total_bytes: 16 * 1024 * 1024 * 1024,
    mem_available_bytes: 1 * 1024 * 1024 * 1024,
  });
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse({ current, history: [current] }),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(
    screen.getByText(/Host needs immediate attention/i),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/Available memory below 10%/i),
  ).toBeInTheDocument();
});

test("shows em-dash for load average when null", () => {
  const current = baseSample({
    load_one: null,
    load_five: null,
    load_fifteen: null,
  });
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse({ current, history: [current] }),
    error: null,
  } as ReturnType<typeof useMachine>);
  render(<MachinePage />);
  expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(1);
});

test("does not render forbidden secret substrings", () => {
  mockUseMachine.mockReturnValue({
    isLoading: false,
    isError: false,
    data: baseResponse(),
    error: null,
  } as ReturnType<typeof useMachine>);
  const { container } = render(<MachinePage />);
  const text = container.textContent ?? "";
  for (const forbidden of [
    "AGENT_SECRET",
    "JWT_SECRET",
    "private_key",
    "relayer_secret",
    "password",
  ]) {
    expect(text).not.toContain(forbidden);
  }
});
