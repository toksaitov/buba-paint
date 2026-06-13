import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useResearchReport,
  useResearchReportJson,
  useResearchReports,
} from "../use-research-reports";

vi.mock("../../lib/research-api", () => ({
  listResearchReports: vi.fn(),
  getResearchReport: vi.fn(),
  getResearchReportJson: vi.fn(),
  getResearchReportCsv: vi.fn(),
}));

import {
  getResearchReport,
  getResearchReportJson,
  listResearchReports,
} from "../../lib/research-api";
const mockListReports = vi.mocked(listResearchReports);
const mockReport = vi.mocked(getResearchReport);
const mockJson = vi.mocked(getResearchReportJson);

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

test("useResearchReports keys on reports and polls every 10s", async () => {
  mockListReports.mockResolvedValue([]);
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchReports(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "reports"] });
  expect(entry?.options.refetchInterval).toBe(10_000);
});

test("useResearchReport is disabled without an id", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchReport(""), { wrapper });
  expect(result.current.isFetching).toBe(false);
  expect(mockReport).not.toHaveBeenCalled();
});

test("useResearchReportJson stays disabled until explicitly enabled", () => {
  const { wrapper } = createWrapper();
  const { result } = renderHook(() => useResearchReportJson("r1"), { wrapper });
  expect(result.current.isFetching).toBe(false);
  expect(mockJson).not.toHaveBeenCalled();
});

test("useResearchReportJson fetches with retry disabled when enabled", async () => {
  mockJson.mockResolvedValue("{}");
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(() => useResearchReportJson("r1", true), {
    wrapper,
  });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const entry = queryClient
    .getQueryCache()
    .find({ queryKey: ["research", "report", "r1", "json"] });
  expect(entry?.options.retry).toBe(false);
});
