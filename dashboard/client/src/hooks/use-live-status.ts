import { useQuery } from "@tanstack/react-query";
import {
  getLiveControlAudit,
  getLiveFills,
  getLiveOrders,
  getLiveReconciliation,
  getLiveRedemptions,
  getLiveSessions,
} from "../lib/api";

export function useLiveSessions(botId: string, limit = 20, enabled = true) {
  return useQuery({
    queryKey: ["live-sessions", botId, limit],
    queryFn: () => getLiveSessions(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId && enabled,
  });
}

export function useLiveOrders(botId: string, limit = 50, enabled = true) {
  return useQuery({
    queryKey: ["live-orders", botId, limit],
    queryFn: () => getLiveOrders(botId, limit),
    refetchInterval: 5000,
    enabled: !!botId && enabled,
  });
}

export function useLiveFills(botId: string, limit = 50, enabled = true) {
  return useQuery({
    queryKey: ["live-fills", botId, limit],
    queryFn: () => getLiveFills(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId && enabled,
  });
}

export function useLiveRedemptions(botId: string, limit = 50, enabled = true) {
  return useQuery({
    queryKey: ["live-redemptions", botId, limit],
    queryFn: () => getLiveRedemptions(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId && enabled,
  });
}

export function useLiveReconciliation(botId: string, limit = 50, enabled = true) {
  return useQuery({
    queryKey: ["live-reconciliation", botId, limit],
    queryFn: () => getLiveReconciliation(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId && enabled,
  });
}

export function useLiveControlAudit(botId: string, limit = 50, enabled = true) {
  return useQuery({
    queryKey: ["live-control-audit", botId, limit],
    queryFn: () => getLiveControlAudit(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId && enabled,
  });
}
