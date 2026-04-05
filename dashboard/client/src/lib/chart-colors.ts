export interface ChartColors {
  background: string;
  textColor: string;
  gridColor: string;
  borderColor: string;
  lineColor: string;
  areaTopColor: string;
  areaBottomColor: string;
  crosshairColor: string;
}

const light: ChartColors = {
  background: "#ffffff",
  textColor: "#666666",
  gridColor: "#f5f5f5",
  borderColor: "#000000",
  lineColor: "#000000",
  areaTopColor: "rgba(0, 0, 0, 0.10)",
  areaBottomColor: "rgba(0, 0, 0, 0.02)",
  crosshairColor: "#000000",
};

const dark: ChartColors = {
  background: "#000000",
  textColor: "#888888",
  gridColor: "#111111",
  borderColor: "#333333",
  lineColor: "#ffffff",
  areaTopColor: "rgba(255, 255, 255, 0.10)",
  areaBottomColor: "rgba(255, 255, 255, 0.02)",
  crosshairColor: "#ffffff",
};

export function getChartColors(isDark: boolean): ChartColors {
  return isDark ? dark : light;
}
