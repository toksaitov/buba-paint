import { useQuery } from "@tanstack/react-query";
import { getEquitySeries } from "../lib/api";

export function useEquitySeries(botId: string, since = 0) {
  return useQuery({
    queryKey: ["equity-series", botId, since],
    queryFn: () => getEquitySeries(botId, since),
    enabled: !!botId,
  });
}
