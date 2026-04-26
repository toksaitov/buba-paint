import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

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

import { useLogs } from "../../hooks/use-logs";
import { LogsPage } from "../logs";

const mockUseLogs = vi.mocked(useLogs);

beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.scrollIntoView = vi.fn();
});

test("shows loading state", () => {
  mockUseLogs.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useLogs>);
  render(<LogsPage />);
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders and filters log lines", async () => {
  const user = userEvent.setup();
  mockUseLogs.mockReturnValue({
    isLoading: false,
    data: {
      lines: [
        "2026-03-20 INFO buba_paint::live: hello world",
        "2026-03-20 WARN buba_paint::live_readonly: readonly shadow runtime rollup",
      ],
    },
  } as ReturnType<typeof useLogs>);

  render(<LogsPage />);

  expect(screen.getByText(/hello world/)).toBeInTheDocument();

  await user.type(screen.getByPlaceholderText("Search log lines"), "readonly");
  expect(screen.queryByText(/hello world/)).not.toBeInTheDocument();
  expect(screen.getByText(/readonly shadow runtime rollup/)).toBeInTheDocument();
});

test("filters by severity, source, and event type and can pause follow mode", async () => {
  const user = userEvent.setup();
  const scrollIntoView = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoView;
  mockUseLogs.mockReturnValue({
    isLoading: false,
    data: {
      lines: [
        "2026-03-20 INFO buba_paint::live: strategy rejection rollup",
        "2026-03-20 WARN buba_paint::clob: reconnecting feed",
        "2026-03-20 ERROR buba_paint::chainlink: feed disconnected",
      ],
    },
  } as ReturnType<typeof useLogs>);

  render(<LogsPage />);

  await user.selectOptions(screen.getByDisplayValue("All event types"), "rollups");
  expect(screen.getByText(/strategy rejection rollup/)).toBeInTheDocument();
  expect(screen.queryByText(/reconnecting feed/)).not.toBeInTheDocument();

  await user.selectOptions(screen.getByDisplayValue("Rollups (1)"), "all");
  await user.selectOptions(screen.getByDisplayValue("All severities"), "warn");
  expect(screen.queryByText(/strategy rejection rollup/)).not.toBeInTheDocument();
  expect(screen.getByText(/reconnecting feed/)).toBeInTheDocument();

  await user.selectOptions(screen.getByDisplayValue("All sources"), "chainlink");
  expect(screen.queryByText(/reconnecting feed/)).not.toBeInTheDocument();

  await user.selectOptions(screen.getByDisplayValue("Warnings"), "all");
  await user.selectOptions(screen.getByDisplayValue("All event types"), "errors");
  expect(screen.getByText(/feed disconnected/)).toBeInTheDocument();

  await user.click(screen.getByLabelText("Follow"));
  scrollIntoView.mockClear();
  await user.selectOptions(screen.getByDisplayValue("Errors (1)"), "errors");
  expect(screen.getByText(/feed disconnected/)).toBeInTheDocument();
  expect(scrollIntoView).not.toHaveBeenCalled();
});

test("shows an empty-state message when filters remove every line", async () => {
  const user = userEvent.setup();
  mockUseLogs.mockReturnValue({
    isLoading: false,
    data: {
      lines: ["2026-03-20 INFO buba_paint::live: hello world"],
    },
  } as ReturnType<typeof useLogs>);

  render(<LogsPage />);

  await user.type(screen.getByPlaceholderText("Search log lines"), "missing");
  expect(screen.getByText("No log lines match the current filters.")).toBeInTheDocument();
});
