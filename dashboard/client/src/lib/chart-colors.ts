export interface ChartColors {
  background: string;
  textColor: string;
  gridColor: string;
  borderColor: string;
  lineColor: string;
  areaTopColor: string;
  areaBottomColor: string;
  crosshairColor: string;
  tooltipBg: string;
  mutedColor: string;
  warningColor: string;
  dangerColor: string;
  seriesPalette: readonly string[];
}

export type ChartTheme = "light" | "dark" | "armed";

const lightPalette = [
  "#1f77b4",
  "#ff7f0e",
  "#2ca02c",
  "#d62728",
  "#9467bd",
  "#8c564b",
  "#17becf",
  "#bcbd22",
] as const;

const darkPalette = [
  "#5fa8d3",
  "#ffb74d",
  "#7bd389",
  "#ef6b6b",
  "#c39bd3",
  "#bf8e6a",
  "#65d6db",
  "#dadc6d",
] as const;

const armedPalette = [
  "#5fffff",
  "#ffe066",
  "#a3e635",
  "#fb7185",
  "#c4b5fd",
  "#fdba74",
  "#67e8f9",
  "#facc15",
] as const;

const light: ChartColors = {
  background: "#ffffff",
  textColor: "#666666",
  gridColor: "#f5f5f5",
  borderColor: "#000000",
  lineColor: "#000000",
  areaTopColor: "rgba(0, 0, 0, 0.10)",
  areaBottomColor: "rgba(0, 0, 0, 0.02)",
  crosshairColor: "#000000",
  tooltipBg: "#ffffff",
  mutedColor: "#e5e5e5",
  warningColor: "#b45309",
  dangerColor: "#b91c1c",
  seriesPalette: lightPalette,
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
  tooltipBg: "#0a0a0a",
  mutedColor: "#1f1f1f",
  warningColor: "#f59e0b",
  dangerColor: "#ef4444",
  seriesPalette: darkPalette,
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
  tooltipBg: "#000060",
  mutedColor: "#1d1d9a",
  warningColor: "#ffe066",
  dangerColor: "#fb7185",
  seriesPalette: armedPalette,
};

export function getChartColors(theme: ChartTheme): ChartColors {
  return theme === "armed" ? armed : theme === "dark" ? dark : light;
}
