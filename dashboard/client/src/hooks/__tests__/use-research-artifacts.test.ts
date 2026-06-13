import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useResearchArtifact,
  useResearchArtifactManifest,
  useResearchArtifacts,
} from "../use-research-artifacts";

vi.mock("../../lib/research-api", () => ({
  listResearchArtifacts: vi.fn(),
  getResearchArtifact: vi.fn(),
  getResearchArtifactManifest: vi.fn(),
  getResearchArtifactChecksums: vi.fn(),
}));

import {
  getResearchArtifact,
  getResearchArtifactManifest,
  listResearchArtifacts,
} from "../../lib/research-api";
const mockListArtifacts = vi.mocked(listResearchArtifacts);
const mockArtifact = vi.mocked(getResearchArtifact);
const mockManifest = vi.mocked(getResearchArtifactManifest);

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

test("useResearchArtifacts keys on artifacts and polls every 10s", async () => {
  mockListArtifacts.mockResolvedValue({ artifacts: [] });
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchArtifacts(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "artifacts"] });
  expect(entry?.options.refetchInterval).toBe(10_000);
});

test("useResearchArtifact is disabled without an id", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchArtifact(""), { wrapper });
  expect(result.current.isFetching).toBe(false);
  expect(mockArtifact).not.toHaveBeenCalled();
});

test("useResearchArtifactManifest stays disabled until explicitly enabled", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchArtifactManifest("a1"), {
    wrapper,
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockManifest).not.toHaveBeenCalled();
});

test("useResearchArtifactManifest does not poll once enabled", async () => {
  mockManifest.mockResolvedValue({} as never);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchArtifactManifest("a1", true), {
    wrapper,
  });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "artifact", "a1", "manifest"] });
  expect(entry?.options.refetchInterval).toBeUndefined();
});
