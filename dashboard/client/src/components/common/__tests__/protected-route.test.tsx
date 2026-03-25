import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { vi, beforeEach } from "vitest";
import { ProtectedRoute } from "../protected-route";
import { useAuthStore } from "../../../stores/auth-store";

beforeEach(() => {
  localStorage.clear();
  useAuthStore.setState({ token: null, user: null });
});

test("renders children when token exists", () => {
  useAuthStore.setState({ token: "valid-token" });

  render(
    <MemoryRouter>
      <ProtectedRoute>
        <div>Secret content</div>
      </ProtectedRoute>
    </MemoryRouter>,
  );

  expect(screen.getByText("Secret content")).toBeDefined();
});

test("redirects to login when no token", () => {
  const { container } = render(
    <MemoryRouter initialEntries={["/dashboard"]}>
      <ProtectedRoute>
        <div>Secret content</div>
      </ProtectedRoute>
    </MemoryRouter>,
  );

  expect(screen.queryByText("Secret content")).toBeNull();
});
