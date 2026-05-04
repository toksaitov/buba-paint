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

export type ChartTheme = "light" | "dark" | "armed";

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

const armed: ChartColors = {
  background: "#000087",
  textColor: "#ffffff",
  gridColor: "#1d1d9a",
  borderColor: "#b0b0e0",
  lineColor: "#5fffff",
  areaTopColor: "rgba(95, 255, 255, 0.40)",
  areaBottomColor: "rgba(95, 255, 255, 0.05)",
  crosshairColor: "#ffffff",
};

export function getChartColors(theme: ChartTheme): ChartColors {
  return theme === "armed" ? armed : theme === "dark" ? dark : light;
}
