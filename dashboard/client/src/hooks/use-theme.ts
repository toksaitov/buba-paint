import { useEffect } from "react";
import { useThemeStore } from "../stores/theme-store";
import { useMediaQuery } from "./use-media-query";

export function useTheme() {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const osDark = useMediaQuery("(prefers-color-scheme: dark)");

  const isDark = mode === "dark" || (mode === "system" && osDark);

  useEffect(() => {
    if (isDark) {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  }, [isDark]);

  return { mode, isDark, setMode };
}
