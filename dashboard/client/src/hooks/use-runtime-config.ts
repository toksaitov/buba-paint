import { useQuery } from "@tanstack/react-query";
import { getRuntimeConfig } from "../lib/api";

export function useRuntimeConfig(botId: string) {
  return useQuery({
    queryKey: ["runtime-config", botId],
    queryFn: () => getRuntimeConfig(botId),
    enabled: !!botId,
    refetchInterval: 10_000,
  });
}
