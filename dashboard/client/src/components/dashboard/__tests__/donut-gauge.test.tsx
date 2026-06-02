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
      width,
      height,
    }: {
      children: React.ReactElement;
      width?: string | number;
      height?: string | number;
    }) => (
      <div
        data-testid="responsive-container"
        data-width={width}
        data-height={height}
      >
        {React.cloneElement(children, { width: Number(width), height: Number(height) })}
      </div>
    ),
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
  expect(getByText("-")).toBeInTheDocument();
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

test("passes explicit dimensions to the chart container", () => {
  const { getByTestId } = render(
    <DonutGauge used={1} total={2} size={72} ariaLabel="sized donut" />,
  );
  expect(getByTestId("responsive-container")).toHaveAttribute(
    "data-width",
    "72",
  );
  expect(getByTestId("responsive-container")).toHaveAttribute(
    "data-height",
    "72",
  );
});

test("clamps used to total without throwing", () => {
  expect(() =>
    render(
      <DonutGauge used={9999} total={100} ariaLabel="clamped donut" />,
    ),
  ).not.toThrow();
});
