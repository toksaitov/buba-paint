import { getChartColors } from "../chart-colors";

test("light theme returns light colors", () => {
  const c = getChartColors("light");
  expect(c.background).toBe("#ffffff");
  expect(c.lineColor).toBe("#000000");
  expect(c.textColor).toBe("#666666");
  expect(c.gridColor).toBe("#f5f5f5");
});

test("dark theme returns dark colors", () => {
  const c = getChartColors("dark");
  expect(c.background).toBe("#000000");
  expect(c.lineColor).toBe("#ffffff");
  expect(c.textColor).toBe("#888888");
  expect(c.gridColor).toBe("#111111");
});

test("armed theme returns vim-blue colors", () => {
  const c = getChartColors("armed");
  expect(c.background).toBe("#000087");
  expect(c.lineColor).toBe("#5fffff");
  expect(c.textColor).toBe("#ffffff");
  expect(c.gridColor).toBe("#1d1d9a");
  expect(c.borderColor).toBe("#b0b0e0");
  expect(c.crosshairColor).toBe("#ffffff");
});

test("returns all required keys", () => {
  const keys = ["background", "textColor", "gridColor", "borderColor", "lineColor", "areaTopColor", "areaBottomColor", "crosshairColor"];
  for (const theme of ["light", "dark", "armed"] as const) {
    const c = getChartColors(theme);
    for (const key of keys) {
      expect(c).toHaveProperty(key);
    }
  }
});
