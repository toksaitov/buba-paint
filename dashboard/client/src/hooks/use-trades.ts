import { useQuery } from "@tanstack/react-query";
import { getTrades } from "../lib/api";

export function useTrades(botId: string, page = 1, perPage = 50) {
  return useQuery({
    queryKey: ["trades", botId, page, perPage],
    queryFn: () => getTrades(botId, page, perPage),
    enabled: !!botId,
  });
}
