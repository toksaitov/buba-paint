import { useQuery } from "@tanstack/react-query";
import {
  getResearchReport,
  getResearchReportCsv,
  getResearchReportJson,
  listResearchReports,
} from "../lib/research-api";

export function useResearchReports() {
  return useQuery({
    queryKey: ["research", "reports"],
    queryFn: listResearchReports,
    refetchInterval: 10_000,
  });
}

export function useResearchReport(id: string) {
  return useQuery({
    queryKey: ["research", "report", id],
    queryFn: () => getResearchReport(id),
    enabled: !!id,
    refetchInterval: 10_000,
  });
}

export function useResearchReportJson(id: string, enabled = false) {
  return useQuery({
    queryKey: ["research", "report", id, "json"],
    queryFn: () => getResearchReportJson(id),
    enabled: !!id && enabled,
    retry: false,
  });
}

export function useResearchReportCsv(id: string, enabled = false) {
  return useQuery({
    queryKey: ["research", "report", id, "csv"],
    queryFn: () => getResearchReportCsv(id),
    enabled: !!id && enabled,
    retry: false,
  });
}
