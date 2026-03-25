import { render } from "@testing-library/react";
import { Logo } from "../logo";

test("renders SVG element", () => {
  const { container } = render(<Logo />);
  const svg = container.querySelector("svg");
  expect(svg).not.toBeNull();
});

test("applies custom size", () => {
  const { container } = render(<Logo size={32} />);
  const svg = container.querySelector("svg");
  expect(svg?.getAttribute("width")).toBe("32");
  expect(svg?.getAttribute("height")).toBe("32");
});
