import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useResearchJobTemplates,
  useResearchQueue,
  useResearchRetention,
} from "../use-research-templates";

vi.mock("../../lib/research-api", () => ({
  listResearchJobTemplates: vi.fn(),
  getResearchQueue: vi.fn(),
  getResearchRetention: vi.fn(),
}));

import {
  getResearchQueue,
  getResearchRetention,
  listResearchJobTemplates,
} from "../../lib/research-api";
const mockTemplates = vi.mocked(listResearchJobTemplates);
const mockQueue = vi.mocked(getResearchQueue);
const mockRetention = vi.mocked(getResearchRetention);

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

test("useResearchJobTemplates keys on job-templates and polls every 10s", async () => {
  mockTemplates.mockResolvedValue({ templates: [] } as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchJobTemplates(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "job-templates"] });
  expect(entry?.options.refetchInterval).toBe(10_000);
});

test("useResearchQueue keys on queue and polls every 5s", async () => {
  mockQueue.mockResolvedValue({} as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchQueue(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "queue"] });
  expect(entry?.options.refetchInterval).toBe(5_000);
});

test("useResearchRetention keys on retention and polls every 10s", async () => {
  mockRetention.mockResolvedValue({} as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchRetention(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "retention"] });
  expect(entry?.options.refetchInterval).toBe(10_000);
});
