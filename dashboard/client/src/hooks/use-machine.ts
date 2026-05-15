import { useQuery } from "@tanstack/react-query";
import { getMachine } from "../lib/api";

export function useMachine(botId: string) {
  return useQuery({
    queryKey: ["machine", botId],
    queryFn: () => getMachine(botId),
    enabled: !!botId,
    refetchInterval: 5_000,
  });
}
