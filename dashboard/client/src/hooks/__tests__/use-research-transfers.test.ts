import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useResearchTransfer,
  useResearchTransfers,
} from "../use-research-transfers";

vi.mock("../../lib/research-api", () => ({
  listArtifactTransfers: vi.fn(),
  getArtifactTransfer: vi.fn(),
}));

import {
  getArtifactTransfer,
  listArtifactTransfers,
} from "../../lib/research-api";
const mockList = vi.mocked(listArtifactTransfers);
const mockGet = vi.mocked(getArtifactTransfer);

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

test("useResearchTransfers keys on transfers and polls every 5s", async () => {
  mockList.mockResolvedValue({ transfers: [] });
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchTransfers(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "transfers"] });
  expect(entry?.options.refetchInterval).toBe(5_000);
});

test("useResearchTransfer slows polling once the transfer is terminal", async () => {
  mockGet.mockResolvedValue({ status: "running" } as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchTransfer("t1"), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "transfer", "t1"] });
  const interval = entry?.options.refetchInterval as (q: unknown) => number;
  expect(interval({ state: { data: undefined } })).toBe(5_000);
  expect(interval({ state: { data: { status: "running" } } })).toBe(3_000);
  expect(interval({ state: { data: { status: "completed" } } })).toBe(10_000);
});
