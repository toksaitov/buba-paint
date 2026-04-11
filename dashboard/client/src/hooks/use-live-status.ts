import { useQuery } from "@tanstack/react-query";
import {
  getLiveFills,
  getLiveOrders,
  getLiveReconciliation,
  getLiveRedemptions,
  getLiveSessions,
  getLiveStatus,
} from "../lib/api";

export function useLiveStatus(botId: string) {
  return useQuery({
    queryKey: ["live-status", botId],
    queryFn: () => getLiveStatus(botId),
    refetchInterval: 5000,
    enabled: !!botId,
  });
}

export function useLiveSessions(botId: string, limit = 20) {
  return useQuery({
    queryKey: ["live-sessions", botId, limit],
    queryFn: () => getLiveSessions(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId,
  });
}

export function useLiveOrders(botId: string, limit = 50) {
  return useQuery({
    queryKey: ["live-orders", botId, limit],
    queryFn: () => getLiveOrders(botId, limit),
    refetchInterval: 5000,
    enabled: !!botId,
  });
}

export function useLiveFills(botId: string, limit = 50) {
  return useQuery({
    queryKey: ["live-fills", botId, limit],
    queryFn: () => getLiveFills(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId,
  });
}

export function useLiveRedemptions(botId: string, limit = 50) {
  return useQuery({
    queryKey: ["live-redemptions", botId, limit],
    queryFn: () => getLiveRedemptions(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId,
  });
}

export function useLiveReconciliation(botId: string, limit = 50) {
  return useQuery({
    queryKey: ["live-reconciliation", botId, limit],
    queryFn: () => getLiveReconciliation(botId, limit),
    refetchInterval: 10000,
    enabled: !!botId,
  });
}
