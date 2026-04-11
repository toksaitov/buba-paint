import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../components/common/protected-route", () => ({
  ProtectedRoute: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

vi.mock("../components/layout/app-shell", async () => {
  const { Outlet } = await import("react-router-dom");
  return {
    AppShell: () => (
      <div data-testid="app-shell">
        <Outlet />
      </div>
    ),
  };
});

vi.mock("../pages/login", () => ({
  LoginPage: () => <div data-testid="login-page">login</div>,
}));

vi.mock("../pages/dashboard", () => ({
  DashboardPage: () => <div data-testid="dashboard-page">dashboard</div>,
}));

vi.mock("../pages/equity", () => ({
  EquityPage: () => <div data-testid="equity-page">equity</div>,
}));

vi.mock("../pages/trades", () => ({
  TradesPage: () => <div data-testid="trades-page">trades</div>,
}));

vi.mock("../pages/signals", () => ({
  SignalsPage: () => <div data-testid="signals-page">signals</div>,
}));

vi.mock("../pages/logs", () => ({
  LogsPage: () => <div data-testid="logs-page">logs</div>,
}));

vi.mock("../pages/stats", () => ({
  StatsPage: () => <div data-testid="stats-page">stats</div>,
}));

vi.mock("../pages/live", () => ({
  LivePage: () => <div data-testid="live-page">live</div>,
}));

import App from "../App";

it("renders the login route directly", () => {
  window.history.pushState({}, "", "/login");

  render(<App />);

  expect(screen.getByTestId("login-page")).toBeInTheDocument();
  expect(screen.queryByTestId("app-shell")).not.toBeInTheDocument();
});

it("renders protected pages inside the app shell", () => {
  window.history.pushState({}, "", "/signals");

  render(<App />);

  expect(screen.getByTestId("app-shell")).toBeInTheDocument();
  expect(screen.getByTestId("signals-page")).toBeInTheDocument();
});

beforeEach(() => {
  window.history.pushState({}, "", "/");
});

afterEach(() => {
  cleanup();
});
