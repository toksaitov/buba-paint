import { renderHook, act } from "@testing-library/react";
import { vi } from "vitest";
import { useTheme } from "../use-theme";
import { useThemeStore } from "../../stores/theme-store";

beforeEach(() => {
  useThemeStore.setState({ mode: "system" });
  document.documentElement.classList.remove("dark");
});

test("system mode with light OS preference does not add dark class", () => {
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(false);
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

test("dark mode adds dark class", () => {
  useThemeStore.setState({ mode: "dark" });
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(true);
  expect(document.documentElement.classList.contains("dark")).toBe(true);
});

test("light mode removes dark class", () => {
  document.documentElement.classList.add("dark");
  useThemeStore.setState({ mode: "light" });
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(false);
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

test("setMode updates the store", () => {
  const { result } = renderHook(() => useTheme());
  act(() => result.current.setMode("dark"));
  expect(useThemeStore.getState().mode).toBe("dark");
});

test("returns current mode", () => {
  useThemeStore.setState({ mode: "light" });
  const { result } = renderHook(() => useTheme());
  expect(result.current.mode).toBe("light");
});
