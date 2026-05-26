import { useQuery } from "@tanstack/react-query";
import {
  getResearchArtifact,
  getResearchArtifactChecksums,
  getResearchArtifactManifest,
  listResearchArtifacts,
} from "../lib/research-api";

export function useResearchArtifacts() {
  return useQuery({
    queryKey: ["research", "artifacts"],
    queryFn: listResearchArtifacts,
    refetchInterval: 10_000,
  });
}

export function useResearchArtifact(id: string) {
  return useQuery({
    queryKey: ["research", "artifact", id],
    queryFn: () => getResearchArtifact(id),
    enabled: !!id,
    refetchInterval: 10_000,
  });
}

export function useResearchArtifactManifest(id: string, enabled = false) {
  return useQuery({
    queryKey: ["research", "artifact", id, "manifest"],
    queryFn: () => getResearchArtifactManifest(id),
    enabled: !!id && enabled,
  });
}

export function useResearchArtifactChecksums(id: string, enabled = false) {
  return useQuery({
    queryKey: ["research", "artifact", id, "checksums"],
    queryFn: () => getResearchArtifactChecksums(id),
    enabled: !!id && enabled,
  });
}
