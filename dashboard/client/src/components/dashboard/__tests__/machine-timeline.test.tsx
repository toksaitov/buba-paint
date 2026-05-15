import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { MachineTimeline, type TimelineSeries } from "../machine-timeline";

vi.mock("recharts", async () => {
  const actual = await vi.importActual<typeof import("recharts")>("recharts");
  return {
    ...actual,
    ResponsiveContainer: ({
      children,
    }: {
      children: React.ReactElement;
    }) =>
      React.cloneElement(children, { width: 600, height: 160 }),
  };
});

import React from "react";

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
  unobserve = vi.fn();
}
vi.stubGlobal("ResizeObserver", ResizeObserverMock);

function makeSeries(): TimelineSeries[] {
  return [
    {
      label: "All cores",
      dataKey: "global",
      values: [
        { ts_ms: 1_000, value: 40 },
        { ts_ms: 2_000, value: 50 },
        { ts_ms: 3_000, value: 45 },
      ],
      color: "#000",
      emphasize: true,
      yAxisFormat: (v) => `${v}%`,
    },
    {
      label: "Core 0",
      dataKey: "core_0",
      values: [
        { ts_ms: 1_000, value: 38 },
        { ts_ms: 2_000, value: 52 },
      ],
      color: "#888",
    },
  ];
}

test("exposes aria-label on the chart wrapper", () => {
  const { getByLabelText } = render(
    <MachineTimeline series={makeSeries()} ariaLabel="CPU history" />,
  );
  expect(getByLabelText("CPU history")).toBeInTheDocument();
});

test("renders without throwing for empty series array", () => {
  expect(() =>
    render(<MachineTimeline series={[]} ariaLabel="empty" />),
  ).not.toThrow();
});

test("renders without throwing when a series has no points", () => {
  const series: TimelineSeries[] = [
    {
      label: "Empty",
      dataKey: "empty",
      values: [],
      color: "#000",
    },
  ];
  expect(() =>
    render(<MachineTimeline series={series} ariaLabel="empty series" />),
  ).not.toThrow();
});

test("respects a custom height prop", () => {
  const { getByLabelText } = render(
    <MachineTimeline series={makeSeries()} height={240} ariaLabel="tall chart" />,
  );
  const wrapper = getByLabelText("tall chart");
  expect(wrapper.getAttribute("style") ?? "").toContain("height: 240");
});
