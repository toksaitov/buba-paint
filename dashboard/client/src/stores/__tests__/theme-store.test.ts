import type { ThemeMode } from "../theme-store";
import { useThemeStore } from "../theme-store";

beforeEach(() => {
  localStorage.removeItem("theme");
  useThemeStore.setState({ mode: "system", armedOverride: false });
});

test("defaults to system mode", () => {
  expect(useThemeStore.getState().mode).toBe("system");
});

test("setMode updates state to dark", () => {
  useThemeStore.getState().setMode("dark");
  expect(useThemeStore.getState().mode).toBe("dark");
});

test("setMode persists to localStorage", () => {
  useThemeStore.getState().setMode("dark");
  expect(localStorage.getItem("theme")).toBe("dark");
});

test("setMode light persists correctly", () => {
  useThemeStore.getState().setMode("light");
  expect(useThemeStore.getState().mode).toBe("light");
  expect(localStorage.getItem("theme")).toBe("light");
});

test("setMode system stores system in localStorage", () => {
  useThemeStore.getState().setMode("dark");
  useThemeStore.getState().setMode("system");
  expect(useThemeStore.getState().mode).toBe("system");
  expect(localStorage.getItem("theme")).toBe("system");
});

test("reads persisted value on init", () => {
  localStorage.setItem("theme", "dark");
  useThemeStore.setState({ mode: (localStorage.getItem("theme") as ThemeMode) ?? "system" });
  expect(useThemeStore.getState().mode).toBe("dark");
});

test("armedOverride defaults to false", () => {
  expect(useThemeStore.getState().armedOverride).toBe(false);
});

test("setArmedOverride flips the runtime flag and is not persisted", () => {
  useThemeStore.getState().setArmedOverride(true);
  expect(useThemeStore.getState().armedOverride).toBe(true);
  expect(localStorage.getItem("theme")).toBeNull();

  useThemeStore.getState().setArmedOverride(false);
  expect(useThemeStore.getState().armedOverride).toBe(false);
});
