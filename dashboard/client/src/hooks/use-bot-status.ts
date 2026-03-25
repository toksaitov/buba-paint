import { useQuery } from "@tanstack/react-query";
import { getBotStatus } from "../lib/api";

export function useBotStatus(botId: string) {
  return useQuery({
    queryKey: ["bot-status", botId],
    queryFn: () => getBotStatus(botId),
    refetchInterval: 5000,
    enabled: !!botId,
  });
}
