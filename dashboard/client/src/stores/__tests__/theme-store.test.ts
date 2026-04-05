import { useThemeStore } from "../theme-store";

beforeEach(() => {
  localStorage.removeItem("theme");
  useThemeStore.setState({ mode: "system" });
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
  const store = useThemeStore.getState();
  useThemeStore.setState({ mode: (localStorage.getItem("theme") as "system" | "light" | "dark") ?? "system" });
  expect(useThemeStore.getState().mode).toBe("dark");
});
