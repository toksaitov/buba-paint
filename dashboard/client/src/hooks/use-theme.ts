import { useEffect } from "react";
import { useThemeStore } from "../stores/theme-store";
import { useMediaQuery } from "./use-media-query";

export function useTheme() {
  const mode = useThemeStore((s) => s.mode);
  const armedOverride = useThemeStore((s) => s.armedOverride);
  const setMode = useThemeStore((s) => s.setMode);
  const osDark = useMediaQuery("(prefers-color-scheme: dark)");

  const userTheme: "light" | "dark" =
    mode === "dark" ? "dark" : mode === "light" ? "light" : osDark ? "dark" : "light";

  const theme: "light" | "dark" | "armed" = armedOverride ? "armed" : userTheme;
  const isDark = theme === "dark";

  useEffect(() => {
    const root = document.documentElement.classList;
    root.remove("dark", "armed");
    if (theme === "dark") root.add("dark");
    else if (theme === "armed") root.add("armed");
  }, [theme]);

  return { mode, theme, isDark, setMode };
}
