import { useQuery } from "@tanstack/react-query";
import {
  getResearchMachine,
  getResearchMachineHealth,
  getResearchMachineTelemetry,
  listResearchMachines,
} from "../lib/research-api";

export function useResearchMachines(enabled = true) {
  return useQuery({
    queryKey: ["research", "machines"],
    queryFn: listResearchMachines,
    enabled,
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

export function useResearchMachineTelemetry(
  id: string,
  enabled = true,
  intervalMs = 5_000,
) {
  return useQuery({
    queryKey: ["research", "machine", id, "telemetry"],
    queryFn: () => getResearchMachineTelemetry(id),
    enabled: !!id && enabled,
    refetchInterval: intervalMs,
  });
}
