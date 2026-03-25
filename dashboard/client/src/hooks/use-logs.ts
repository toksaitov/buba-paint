import { useQuery } from "@tanstack/react-query";
import { getLogs } from "../lib/api";

export function useLogs(botId: string, lines = 200) {
  return useQuery({
    queryKey: ["logs", botId, lines],
    queryFn: () => getLogs(botId, lines),
    enabled: !!botId,
    refetchInterval: 5000,
  });
}
