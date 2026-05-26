import { useQuery } from "@tanstack/react-query";
import {
  getResearchMachine,
  getResearchMachineHealth,
  listResearchMachines,
} from "../lib/research-api";

export function useResearchMachines() {
  return useQuery({
    queryKey: ["research", "machines"],
    queryFn: listResearchMachines,
    refetchInterval: 10_000,
  });
}

export function useResearchMachine(id: string) {
  return useQuery({
    queryKey: ["research", "machine", id],
    queryFn: () => getResearchMachine(id),
    enabled: !!id,
    refetchInterval: 10_000,
  });
}

export function useResearchMachineHealth(id: string, enabled = true) {
  return useQuery({
    queryKey: ["research", "machine", id, "health"],
    queryFn: () => getResearchMachineHealth(id),
    enabled: !!id && enabled,
    refetchInterval: 5_000,
  });
}
