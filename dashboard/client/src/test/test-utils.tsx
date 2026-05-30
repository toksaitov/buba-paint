import { render, type RenderOptions } from "@testing-library/react";
import type { ReactElement } from "react";
import { TestProviders } from "./test-provider";

export function renderWithProviders(
  ui: ReactElement,
  options?: Omit<RenderOptions, "wrapper">,
) {
  return render(ui, { wrapper: TestProviders, ...options });
}
