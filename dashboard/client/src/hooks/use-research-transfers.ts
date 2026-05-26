import { useQuery } from "@tanstack/react-query";
import {
  getArtifactTransfer,
  listArtifactTransfers,
} from "../lib/research-api";
import { isTransferTerminal } from "../lib/research-permissions";

export function useResearchTransfers() {
  return useQuery({
    queryKey: ["research", "transfers"],
    queryFn: listArtifactTransfers,
    refetchInterval: 5_000,
  });
}

export function useResearchTransfer(id: string) {
  return useQuery({
    queryKey: ["research", "transfer", id],
    queryFn: () => getArtifactTransfer(id),
    enabled: !!id,
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data) return 5_000;
      return isTransferTerminal(data.status) ? 10_000 : 3_000;
    },
  });
}
