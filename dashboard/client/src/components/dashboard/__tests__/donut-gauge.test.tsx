import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import React from "react";
import { DonutGauge } from "../donut-gauge";

vi.mock("recharts", async () => {
  const actual = await vi.importActual<typeof import("recharts")>("recharts");
  return {
    ...actual,
    ResponsiveContainer: ({
      children,
    }: {
      children: React.ReactElement;
    }) =>
      React.cloneElement(children, { width: 64, height: 64 }),
  };
});

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
  unobserve = vi.fn();
}
vi.stubGlobal("ResizeObserver", ResizeObserverMock);

test("renders the used percentage in the middle", () => {
  const { getByText } = render(
    <DonutGauge used={42} total={100} ariaLabel="cpu donut" />,
  );
  expect(getByText("42%")).toBeInTheDocument();
});

test("renders em-dash when total is zero", () => {
  const { getByText } = render(
    <DonutGauge used={0} total={0} ariaLabel="no-data donut" />,
  );
  expect(getByText("—")).toBeInTheDocument();
});

test("exposes ariaLabel for screen readers", () => {
  const { getByLabelText } = render(
    <DonutGauge used={1} total={2} ariaLabel="memory donut" />,
  );
  expect(getByLabelText("memory donut")).toBeInTheDocument();
});

test("renders the optional caption label", () => {
  const { getByText } = render(
    <DonutGauge used={1} total={4} label="DISK" ariaLabel="disk donut" />,
  );
  expect(getByText("DISK")).toBeInTheDocument();
});

test("clamps used to total without throwing", () => {
  expect(() =>
    render(
      <DonutGauge used={9999} total={100} ariaLabel="clamped donut" />,
    ),
  ).not.toThrow();
});
