import { useQuery } from "@tanstack/react-query";
import { getBotProcessStatus } from "../lib/api";

export function useProcessStatus(botId: string) {
  return useQuery({
    queryKey: ["process-status", botId],
    queryFn: () => getBotProcessStatus(botId),
    refetchInterval: 5000,
    enabled: !!botId,
  });
}
