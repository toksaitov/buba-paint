import { useQuery } from "@tanstack/react-query";
import { getBalance } from "../lib/api";

export function useBalance(botId: string, since = 0) {
  return useQuery({
    queryKey: ["balance", botId, since],
    queryFn: () => getBalance(botId, since),
    enabled: !!botId,
  });
}
