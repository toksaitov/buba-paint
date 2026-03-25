import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { vi, beforeEach } from "vitest";

const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mockNavigate,
    Navigate: ({ to }: { to: string }) => <div data-testid="navigate" data-to={to} />,
  };
});

vi.mock("../../lib/api", () => ({
  login: vi.fn(),
  getMe: vi.fn().mockResolvedValue({ id: "1", username: "u", role: "admin" }),
}));

import { LoginPage } from "../login";
import { useAuthStore } from "../../stores/auth-store";
import * as api from "../../lib/api";
const mockLogin = vi.mocked(api.login);

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useAuthStore.setState({ token: null, user: null });
});

function getInputs() {
  const username = document.querySelector('input[type="text"]') as HTMLInputElement;
  const password = document.querySelector('input[type="password"]') as HTMLInputElement;
  return { username, password };
}

test("renders form with username, password, and submit", () => {
  render(<LoginPage />);
  const { username, password } = getInputs();
  expect(username).not.toBeNull();
  expect(password).not.toBeNull();
  expect(screen.getByRole("button", { name: "Sign in" })).toBeDefined();
});

test("redirects when already logged in", () => {
  useAuthStore.setState({ token: "existing" });
  render(<LoginPage />);
  const nav = screen.getByTestId("navigate");
  expect(nav.getAttribute("data-to")).toBe("/");
});

test("shows error on failed login", async () => {
  mockLogin.mockRejectedValue(new Error("bad creds"));
  render(<LoginPage />);

  const { username, password } = getInputs();
  fireEvent.change(username, { target: { value: "admin" } });
  fireEvent.change(password, { target: { value: "wrong" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

  await waitFor(() => {
    expect(screen.getByText("Invalid credentials")).toBeDefined();
  });
});

test("disables button while loading", async () => {
  mockLogin.mockReturnValue(new Promise(() => {}));
  render(<LoginPage />);

  const { username, password } = getInputs();
  fireEvent.change(username, { target: { value: "u" } });
  fireEvent.change(password, { target: { value: "p" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

  await waitFor(() => {
    expect(screen.getByRole("button", { name: "Signing in..." })).toBeDefined();
    expect((screen.getByRole("button", { name: "Signing in..." }) as HTMLButtonElement).disabled).toBe(true);
  });
});

test("calls login with form values", async () => {
  mockLogin.mockResolvedValue({ token: "t", user: { id: "1", username: "u", role: "admin" } });
  render(<LoginPage />);

  const { username, password } = getInputs();
  fireEvent.change(username, { target: { value: "admin" } });
  fireEvent.change(password, { target: { value: "secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

  await waitFor(() => {
    expect(mockLogin).toHaveBeenCalledWith("admin", "secret");
  });
});
