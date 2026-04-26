import { useQuery } from "@tanstack/react-query";
import { getTradingSummary } from "../lib/api";

export function useTradingSummary(botId: string) {
  return useQuery({
    queryKey: ["trading-summary", botId],
    queryFn: () => getTradingSummary(botId),
    refetchInterval: 5000,
    enabled: !!botId,
  });
}
