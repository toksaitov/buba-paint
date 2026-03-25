import { useQuery } from "@tanstack/react-query";
import { getSignals } from "../lib/api";

export function useSignals(botId: string, limit = 100) {
  return useQuery({
    queryKey: ["signals", botId, limit],
    queryFn: () => getSignals(botId, limit),
    enabled: !!botId,
  });
}
