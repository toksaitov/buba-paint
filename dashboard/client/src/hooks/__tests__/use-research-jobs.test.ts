import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import { useResearchJob, useResearchJobs } from "../use-research-jobs";
import {
  fixtureJobCompleted,
  fixtureJobRunning,
} from "../../lib/research-fixtures";

vi.mock("../../lib/research-api", () => ({
  listResearchJobs: vi.fn(),
  getResearchJob: vi.fn(),
}));

import { getResearchJob, listResearchJobs } from "../../lib/research-api";
const mockList = vi.mocked(listResearchJobs);
const mockGet = vi.mocked(getResearchJob);

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

test("useResearchJobs uses the jobs key and a 5s poll", async () => {
  mockList.mockResolvedValue({ jobs: [] });
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchJobs(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "jobs"] });
  expect(entry).toBeDefined();
  expect(entry?.options.refetchInterval).toBe(5_000);
  expect(mockList).toHaveBeenCalledTimes(1);
});

test("useResearchJob is disabled without an id", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchJob(""), { wrapper });
  expect(result.current.isFetching).toBe(false);
  expect(mockGet).not.toHaveBeenCalled();
});

test("useResearchJob polls fast while active and slow once terminal", async () => {
  mockGet.mockResolvedValue(fixtureJobRunning());
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchJob("fixture-job-running"), {
    wrapper,
  });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "job", "fixture-job-running"] });
  const interval = entry?.options.refetchInterval as (q: unknown) => number;
  expect(typeof interval).toBe("function");
  expect(interval({ state: { data: undefined } })).toBe(5_000);
  expect(interval({ state: { data: fixtureJobRunning() } })).toBe(3_000);
  expect(interval({ state: { data: fixtureJobCompleted() } })).toBe(10_000);
});
