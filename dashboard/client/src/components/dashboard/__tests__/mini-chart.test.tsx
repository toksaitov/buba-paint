import { render, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { MiniChart } from "../mini-chart";
import type { BalanceEntry } from "../../../lib/types";

const chartMocks = vi.hoisted(() => {
  const setData = vi.fn();
  const fitContent = vi.fn();
  const applyOptions = vi.fn();
  const remove = vi.fn();
  const addSeries = vi.fn(() => ({ setData }));
  const timeScale = vi.fn(() => ({ fitContent }));
  const createChart = vi.fn(() => ({
    addSeries,
    timeScale,
    applyOptions,
    remove,
  }));

  return {
    addSeries,
    applyOptions,
    createChart,
    fitContent,
    remove,
    setData,
    timeScale,
  };
});

vi.mock("lightweight-charts", () => ({
  AreaSeries: Symbol("AreaSeries"),
  createChart: chartMocks.createChart,
}));

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
}

beforeEach(() => {
  chartMocks.createChart.mockClear();
  chartMocks.addSeries.mockClear();
  chartMocks.setData.mockClear();
  chartMocks.fitContent.mockClear();
  chartMocks.applyOptions.mockClear();
  chartMocks.remove.mockClear();
  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
});

it("deduplicates and sorts chart data before rendering", async () => {
  const entries: BalanceEntry[] = [
    { id: 1, timestamp: 3_000, event: "tick", balance: 101 },
    { id: 2, timestamp: 1_000, event: "tick", balance: 99 },
    { id: 3, timestamp: 3_001, event: "tick", balance: 103 },
  ];

  const { unmount, getByText } = render(<MiniChart entries={entries} />);

  await waitFor(() => {
    expect(chartMocks.setData).toHaveBeenCalledWith([
      { time: 1, value: 99 },
      { time: 3, value: 103 },
    ]);
  });

  expect(getByText("Equity Curve")).toBeInTheDocument();
  expect(chartMocks.fitContent).toHaveBeenCalled();

  unmount();

  expect(chartMocks.remove).toHaveBeenCalled();
});

