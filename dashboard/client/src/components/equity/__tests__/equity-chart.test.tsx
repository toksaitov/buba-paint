import { render, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { EquityChart } from "../equity-chart";
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

it("normalizes entries and tears down the chart on unmount", async () => {
  const entries: BalanceEntry[] = [
    { id: 1, timestamp: 2_000, event: "tick", balance: 105 },
    { id: 2, timestamp: 1_000, event: "tick", balance: 100 },
    { id: 3, timestamp: 2_400, event: "tick", balance: 111 },
  ];

  const { container, unmount } = render(<EquityChart entries={entries} />);

  await waitFor(() => {
    expect(chartMocks.setData).toHaveBeenCalledWith([
      { time: 1, value: 100 },
      { time: 2, value: 111 },
    ]);
  });

  expect(container.firstChild).toHaveClass("w-full");
  expect(chartMocks.fitContent).toHaveBeenCalled();

  unmount();

  expect(chartMocks.remove).toHaveBeenCalled();
});

