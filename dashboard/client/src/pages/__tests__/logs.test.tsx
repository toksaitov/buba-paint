import { render, screen } from "@testing-library/react";
import { vi, beforeEach } from "vitest";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-logs", () => ({
  useLogs: vi.fn(),
}));

vi.mock("ansi-to-react", () => ({
  default: ({ children }: { children: string }) => <span>{children}</span>,
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { LogsPage } from "../logs";
import { useLogs } from "../../hooks/use-logs";
const mockUseLogs = vi.mocked(useLogs);

beforeEach(() => {
  vi.clearAllMocks();
  // jsdom doesn't implement scrollIntoView
  Element.prototype.scrollIntoView = vi.fn();
});

test("shows loading state", () => {
  mockUseLogs.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useLogs>);
  render(<LogsPage />);
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders data when loaded", () => {
  mockUseLogs.mockReturnValue({
    isLoading: false,
    data: { lines: ["2026-03-20 hello world", "2026-03-20 second line"] },
  } as ReturnType<typeof useLogs>);
  render(<LogsPage />);
  expect(screen.getByText("Bot Log")).toBeDefined();
  expect(screen.getByText(/hello world/)).toBeDefined();
});
