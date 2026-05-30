import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, beforeEach } from "vitest";

const mockNavigate = vi.fn();
vi.mock("react-router-dom", () => ({
  useNavigate: () => mockNavigate,
}));

vi.mock("../../lib/api", () => ({
  login: vi.fn(),
  getMe: vi.fn(),
}));

import { useAuth } from "../use-auth";
import { useAuthStore } from "../../stores/auth-store";
import * as api from "../../lib/api";

const mockLogin = vi.mocked(api.login);
const mockGetMe = vi.mocked(api.getMe);

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useAuthStore.setState({ token: null, user: null });
});

test("doLogin calls API and sets auth", async () => {
  const user = { id: "1", username: "admin", role: "admin" };
  mockLogin.mockResolvedValue({ token: "jwt-token", user });

  const { result } = renderHook(() => useAuth());

  await act(async () => {
    await result.current.login("admin", "pass");
  });

  const state = useAuthStore.getState();
  expect(state.token).toBe("jwt-token");
  expect(state.user).toEqual(user);
});

test("doLogin navigates to / on success", async () => {
  mockLogin.mockResolvedValue({
    token: "t",
    user: { id: "1", username: "u", role: "admin" },
  });

  const { result } = renderHook(() => useAuth());

  await act(async () => {
    await result.current.login("u", "p");
  });

  expect(mockNavigate).toHaveBeenCalledWith("/");
});

test("doLogout clears store and navigates to /login", () => {
  useAuthStore.getState().setAuth("tok", { id: "1", username: "u", role: "admin" });

  const { result } = renderHook(() => useAuth());

  act(() => {
    result.current.logout();
  });

  expect(useAuthStore.getState().token).toBeNull();
  expect(mockNavigate).toHaveBeenCalledWith("/login");
});

test("fetches user on mount when token exists", async () => {
  const user = { id: "1", username: "admin", role: "admin" };
  useAuthStore.setState({ token: "existing-token", user: null });
  mockGetMe.mockResolvedValue(user);

  renderHook(() => useAuth());

  await waitFor(() => expect(mockGetMe).toHaveBeenCalled());
  await waitFor(() => expect(useAuthStore.getState().user).toEqual(user));
});

test("logs out if getMe fails", async () => {
  useAuthStore.setState({ token: "bad-token", user: null });
  mockGetMe.mockRejectedValue(new Error("unauthorized"));

  renderHook(() => useAuth());

  await waitFor(() => expect(mockGetMe).toHaveBeenCalled());
  await waitFor(() => expect(useAuthStore.getState().token).toBeNull());
});

test("isLoggedIn reflects token presence", () => {
  const { result, rerender } = renderHook(() => useAuth());
  expect(result.current.isLoggedIn).toBe(false);

  act(() => {
    useAuthStore.getState().setAuth("tok", { id: "1", username: "u", role: "admin" });
  });
  rerender();
  expect(result.current.isLoggedIn).toBe(true);
});
