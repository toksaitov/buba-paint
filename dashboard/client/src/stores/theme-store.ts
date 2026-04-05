import { create } from "zustand";

type ThemeMode = "system" | "light" | "dark";

interface ThemeState {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
}

function readPersistedMode(): ThemeMode {
  const stored = localStorage.getItem("theme");
  if (stored === "dark" || stored === "light") return stored;
  return "system";
}

export const useThemeStore = create<ThemeState>((set) => ({
  mode: readPersistedMode(),
  setMode: (mode) => {
    localStorage.setItem("theme", mode);
    set({ mode });
  },
}));
