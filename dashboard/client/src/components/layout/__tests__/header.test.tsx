import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { Header } from "../header";
import { renderWithProviders } from "../../../test/test-utils";
import { useAuthStore } from "../../../stores/auth-store";

vi.mock("../../../lib/api", () => ({
  getBotProcessStatus: vi.fn(),
  botStart: vi.fn(),
  botStop: vi.fn(),
  botRestart: vi.fn(),
}));

import {
  getBotProcessStatus,
  botStart,
  botStop,
  botRestart,
} from "../../../lib/api";

const mockGetProcessStatus = vi.mocked(getBotProcessStatus);
const mockBotStart = vi.mocked(botStart);
const mockBotStop = vi.mocked(botStop);
const mockBotRestart = vi.mocked(botRestart);

beforeEach(() => {
  useAuthStore.setState({
    token: "test-token",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockGetProcessStatus.mockResolvedValue({
    active: true,
    pid: 42,
    uptime_secs: 120,
  });
  mockBotStart.mockResolvedValue({ active: true, pid: 43, uptime_secs: 0 });
  mockBotStop.mockResolvedValue({ active: false, pid: null, uptime_secs: null });
  mockBotRestart.mockResolvedValue({ active: true, pid: 44, uptime_secs: 0 });
});

afterEach(() => {
  vi.clearAllMocks();
});

function renderHeader() {
  return renderWithProviders(
    <Header
      bot={{ id: "bot-1", name: "Test Bot" }}
      botId="bot-1"
      collapsed={false}
      onToggle={() => {}}
    />,
  );
}

test("shows Running badge when process is active", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
});

test("shows Stopped badge when process is inactive", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());
});

test("Start button disabled when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByTitle("Bot is already running")).toBeDisabled();
});

test("Stop button disabled when process is stopped", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());

  const disabledButtons = screen.getAllByTitle("Bot is not running");
  expect(disabledButtons.length).toBe(2);
  disabledButtons.forEach((btn) => expect(btn).toBeDisabled());
});

test("Start button enabled when process is stopped", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());
  expect(screen.getByTitle("Start bot")).toBeEnabled();
});

test("Stop button enabled when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByTitle("Stop bot")).toBeEnabled();
});

test("Stop button calls botStop", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());

  const stopBtn = screen.getByTitle("Stop bot");
  await userEvent.click(stopBtn);
  await waitFor(() => expect(mockBotStop).toHaveBeenCalledWith("bot-1"));
});

test("shows error toast on action failure", async () => {
  mockBotStop.mockRejectedValue(new Error("something broke"));
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());

  const stopBtn = screen.getByTitle("Stop bot");
  await userEvent.click(stopBtn);
  await waitFor(() =>
    expect(screen.getByText("something broke")).toBeInTheDocument(),
  );
});

test("error toast dismisses on X click", async () => {
  mockBotStop.mockRejectedValue(new Error("test error"));
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());

  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() =>
    expect(screen.getByText("test error")).toBeInTheDocument(),
  );

  const errorBanner = screen.getByText("test error").closest("div")!;
  const dismissBtn = errorBanner.querySelector("button")!;
  await userEvent.click(dismissBtn);
  expect(screen.queryByText("test error")).not.toBeInTheDocument();
});

test("shows uptime when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByText("2m")).toBeInTheDocument();
});

test("displays bot name", () => {
  renderHeader();
  expect(screen.getByText("Test Bot")).toBeInTheDocument();
});

