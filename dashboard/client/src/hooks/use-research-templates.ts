import { useQuery } from "@tanstack/react-query";
import {
  getResearchQueue,
  getResearchRetention,
  listResearchJobTemplates,
} from "../lib/research-api";

export function useResearchJobTemplates() {
  return useQuery({
    queryKey: ["research", "job-templates"],
    queryFn: listResearchJobTemplates,
    refetchInterval: 10_000,
  });
}

export function useResearchQueue() {
  return useQuery({
    queryKey: ["research", "queue"],
    queryFn: getResearchQueue,
    refetchInterval: 5_000,
  });
}

export function useResearchRetention() {
  return useQuery({
    queryKey: ["research", "retention"],
    queryFn: getResearchRetention,
    refetchInterval: 10_000,
  });
}
