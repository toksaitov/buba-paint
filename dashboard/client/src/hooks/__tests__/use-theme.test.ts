import { renderHook, act } from "@testing-library/react";
import { useTheme } from "../use-theme";
import { useThemeStore } from "../../stores/theme-store";

beforeEach(() => {
  useThemeStore.setState({ mode: "system", armedOverride: false });
  document.documentElement.classList.remove("dark");
  document.documentElement.classList.remove("armed");
});

test("system mode with light OS preference does not add dark class", () => {
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(false);
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(document.documentElement.classList.contains("armed")).toBe(false);
});

test("dark mode adds dark class", () => {
  useThemeStore.setState({ mode: "dark" });
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(true);
  expect(result.current.theme).toBe("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  expect(document.documentElement.classList.contains("armed")).toBe(false);
});

test("light mode removes dark class", () => {
  document.documentElement.classList.add("dark");
  useThemeStore.setState({ mode: "light" });
  const { result } = renderHook(() => useTheme());
  expect(result.current.isDark).toBe(false);
  expect(result.current.theme).toBe("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

test("armedOverride forces armed class regardless of mode", () => {
  useThemeStore.setState({ mode: "light", armedOverride: true });
  const { result } = renderHook(() => useTheme());
  expect(result.current.theme).toBe("armed");
  expect(result.current.isDark).toBe(false);
  expect(document.documentElement.classList.contains("armed")).toBe(true);
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

test("clearing armedOverride reverts to user preference", () => {
  useThemeStore.setState({ mode: "dark", armedOverride: true });
  const { result, rerender } = renderHook(() => useTheme());
  expect(document.documentElement.classList.contains("armed")).toBe(true);

  act(() => useThemeStore.getState().setArmedOverride(false));
  rerender();

  expect(document.documentElement.classList.contains("armed")).toBe(false);
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  expect(result.current.theme).toBe("dark");
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
