import { getChartColors } from "../chart-colors";

test("light theme returns light colors", () => {
  const c = getChartColors(false);
  expect(c.background).toBe("#ffffff");
  expect(c.lineColor).toBe("#000000");
  expect(c.textColor).toBe("#666666");
  expect(c.gridColor).toBe("#f5f5f5");
});

test("dark theme returns dark colors", () => {
  const c = getChartColors(true);
  expect(c.background).toBe("#000000");
  expect(c.lineColor).toBe("#ffffff");
  expect(c.textColor).toBe("#888888");
  expect(c.gridColor).toBe("#111111");
});

test("returns all required keys", () => {
  const keys = ["background", "textColor", "gridColor", "borderColor", "lineColor", "areaTopColor", "areaBottomColor", "crosshairColor"];
  for (const isDark of [true, false]) {
    const c = getChartColors(isDark);
    for (const key of keys) {
      expect(c).toHaveProperty(key);
    }
  }
});
