import { create } from "zustand";

export type ThemeMode = "system" | "light" | "dark";

interface ThemeState {
  mode: ThemeMode;
  armedOverride: boolean;
  setMode: (mode: ThemeMode) => void;
  setArmedOverride: (armed: boolean) => void;
}

function readPersistedMode(): ThemeMode {
  const stored = localStorage.getItem("theme");
  if (stored === "dark" || stored === "light") return stored;
  return "system";
}

export const useThemeStore = create<ThemeState>((set) => ({
  mode: readPersistedMode(),
  armedOverride: false,
  setMode: (mode) => {
    localStorage.setItem("theme", mode);
    set({ mode });
  },
  setArmedOverride: (armedOverride) => set({ armedOverride }),
}));
