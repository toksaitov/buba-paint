import { useQuery } from "@tanstack/react-query";
import {
  getResearchJob,
  listResearchJobEvents,
  listResearchJobs,
} from "../lib/research-api";
import { isJobTerminal } from "../lib/research-permissions";

export function useResearchJobs() {
  return useQuery({
    queryKey: ["research", "jobs"],
    queryFn: listResearchJobs,
    refetchInterval: 5_000,
  });
}

export function useResearchJob(id: string) {
  return useQuery({
    queryKey: ["research", "job", id],
    queryFn: () => getResearchJob(id),
    enabled: !!id,
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data) return 5_000;
      return isJobTerminal(data.job.status) ? 10_000 : 3_000;
    },
  });
}

export function useResearchJobEvents(id: string, enabled = false) {
  return useQuery({
    queryKey: ["research", "job", id, "events"],
    queryFn: () => listResearchJobEvents(id),
    enabled: !!id && enabled,
    refetchInterval: 5_000,
  });
}
