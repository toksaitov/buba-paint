import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useMachine } from "../use-machine";
import type { ReactNode } from "react";
import { createElement } from "react";
import type { MachineResponse } from "../../lib/types";

vi.mock("../../lib/api", () => ({
  getMachine: vi.fn(),
}));

import { getMachine } from "../../lib/api";
const mockGetMachine = vi.mocked(getMachine);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

function emptyResponse(): MachineResponse {
  return {
    host: {
      hostname: "h",
      os_name: "o",
      os_version: "1",
      kernel_version: "1",
      cpu_count: 1,
      total_ram_bytes: 0,
    },
    agent_started_at_ms: 0,
    current: null,
    history: [],
    runtime_db: { db_path: "/", db_bytes: null, wal_bytes: null, shm_bytes: null },
    sampler: { sample_interval_ms: 5000, samples_collected: 0, last_error: null },
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

test("returns data on success", async () => {
  mockGetMachine.mockResolvedValue(emptyResponse());
  const { result } = renderHook(() => useMachine("paint"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.sampler.sample_interval_ms).toBe(5000);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useMachine(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetMachine).not.toHaveBeenCalled();
});

test("propagates fetch errors", async () => {
  mockGetMachine.mockRejectedValue(new Error("network down"));
  const { result } = renderHook(() => useMachine("paint"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("network down");
});
