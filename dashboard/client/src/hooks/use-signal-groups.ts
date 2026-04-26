import { useQuery } from "@tanstack/react-query";
import { getSignalGroups } from "../lib/api";

export function useSignalGroups(botId: string, limit = 50, quietGapMs = 5000) {
  return useQuery({
    queryKey: ["signal-groups", botId, limit, quietGapMs],
    queryFn: () => getSignalGroups(botId, limit, quietGapMs),
    enabled: !!botId,
  });
}
