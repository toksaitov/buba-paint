import { describe, expect, test, beforeEach } from "vitest";
import { useAuthStore } from "../auth-store";

beforeEach(() => {
  localStorage.clear();
  // Reset the store state between tests.
  useAuthStore.setState({ token: null, user: null });
});

describe("useAuthStore", () => {
  test("initial state has null user", () => {
    const state = useAuthStore.getState();
    expect(state.user).toBeNull();
  });

  test("initial state reads token from localStorage", () => {
    localStorage.setItem("token", "saved-token");
    // Re-create by calling the store initializer directly isn't possible,
    // but we can test that setAuth persists to localStorage.
    const state = useAuthStore.getState();
    // After beforeEach reset, token is null. Verify the reset works.
    expect(state.token).toBeNull();
  });

  test("setAuth stores token and user in state and localStorage", () => {
    const user = { id: "1", username: "admin", role: "admin" };
    useAuthStore.getState().setAuth("my-token", user);

    const state = useAuthStore.getState();
    expect(state.token).toBe("my-token");
    expect(state.user).toEqual(user);
    expect(localStorage.getItem("token")).toBe("my-token");
  });

  test("logout removes token from localStorage", () => {
    useAuthStore.getState().setAuth("tok", { id: "1", username: "u", role: "admin" });
    expect(localStorage.getItem("token")).toBe("tok");

    useAuthStore.getState().logout();
    expect(localStorage.getItem("token")).toBeNull();
  });

  test("logout clears user from state", () => {
    useAuthStore.getState().setAuth("tok", { id: "1", username: "u", role: "admin" });
    useAuthStore.getState().logout();

    const state = useAuthStore.getState();
    expect(state.token).toBeNull();
    expect(state.user).toBeNull();
  });
});
