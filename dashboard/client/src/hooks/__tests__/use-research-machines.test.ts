import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useResearchMachineHealth,
  useResearchMachineTelemetry,
  useResearchMachines,
} from "../use-research-machines";

vi.mock("../../lib/research-api", () => ({
  listResearchMachines: vi.fn(),
  getResearchMachine: vi.fn(),
  getResearchMachineHealth: vi.fn(),
  getResearchMachineTelemetry: vi.fn(),
}));

import {
  getResearchMachineHealth,
  getResearchMachineTelemetry,
  listResearchMachines,
} from "../../lib/research-api";
const mockListMachines = vi.mocked(listResearchMachines);
const mockHealth = vi.mocked(getResearchMachineHealth);
const mockTelemetry = vi.mocked(getResearchMachineTelemetry);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

afterEach(() => {
  vi.clearAllMocks();
});

test("useResearchMachines keys on machines and polls every 10s", async () => {
  mockListMachines.mockResolvedValue({ machines: [] });
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchMachines(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "machines"] });
  expect(entry?.options.refetchInterval).toBe(10_000);
});

test("useResearchMachines respects the enabled flag", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchMachines(false), { wrapper });
  expect(result.current.isFetching).toBe(false);
  expect(mockListMachines).not.toHaveBeenCalled();
});

test("useResearchMachineHealth requires both an id and the enabled flag", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchMachineHealth("m1", false), {
    wrapper,
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockHealth).not.toHaveBeenCalled();
});

test("useResearchMachineTelemetry keys per machine and honors the interval override", async () => {
  mockTelemetry.mockResolvedValue({} as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(
    () => useResearchMachineTelemetry("m1", true, 2_000),
    { wrapper },
  );
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "machine", "m1", "telemetry"] });
  expect(entry?.options.refetchInterval).toBe(2_000);
});
