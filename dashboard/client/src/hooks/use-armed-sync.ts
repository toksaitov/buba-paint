import { useEffect } from "react";
import { useThemeStore } from "../stores/theme-store";
import { useTradingSummary } from "./use-trading-summary";

export function useArmedSync(botId: string) {
  const { data } = useTradingSummary(botId);
  const setArmedOverride = useThemeStore((s) => s.setArmedOverride);
  const armed = data?.trading_state === "armed";

  useEffect(() => {
    setArmedOverride(armed);
  }, [armed, setArmedOverride]);
}
