import type {
  LiveControlAction,
  RealAccountSummary,
  TradingCapabilities,
  TradingControlCapability,
} from "./types";

export type ControlActionGroup = "dryrun" | "reversible" | "destructive";

export interface ControlActionConfig {
  action: LiveControlAction;
  label: string;
  group: ControlActionGroup;
  tone: "neutral" | "danger";
  desktopOnly?: boolean;
  requiresExtraCheckbox?: boolean;
  consequence: string;
  effect: string;
  confirmation?: (botId: string, fingerprintPrefix: string) => string;
}

export const controlActions: ControlActionConfig[] = [
  {
    action: "preflight",
    label: "Preflight",
    group: "dryrun",
    tone: "neutral",
    consequence: "Pre-trade gate check on feeds, account, and allowance.",
    effect: "Read-only. No venue state changes, no orders placed.",
  },
  {
    action: "arm",
    label: "Arm",
    group: "reversible",
    tone: "neutral",
    desktopOnly: true,
    consequence: "Enables strategies to place real orders on Polymarket.",
    effect: "Reversible. Strategies act within seconds. Disarm or Stop After Flat to halt.",
    confirmation: (botId, fingerprintPrefix) => `ARM "${botId}" ${fingerprintPrefix}`,
  },
  {
    action: "disarm",
    label: "Disarm",
    group: "reversible",
    tone: "neutral",
    consequence: "Halts new order placement.",
    effect: "Open orders and positions stay. Use Cancel All or Redeem All separately.",
  },
  {
    action: "stop_after_flat",
    label: "Stop After Flat",
    group: "reversible",
    tone: "neutral",
    consequence: "Halts new orders once open exposure has closed out.",
    effect: "Strategies finish their cycles, then pause.",
  },
  {
    action: "cancel_all",
    label: "Cancel All",
    group: "destructive",
    tone: "danger",
    consequence: "Cancels every open venue order immediately.",
    effect: "Open positions remain. Cannot be undone.",
    confirmation: (botId) => `CANCEL ALL "${botId}"`,
  },
  {
    action: "redeem_all",
    label: "Redeem All",
    group: "destructive",
    tone: "danger",
    desktopOnly: true,
    consequence: "Redeems all resolved positions to USDC.",
    effect: "Triggers an on-chain transaction. Cannot be undone.",
    confirmation: (botId) => `REDEEM ALL "${botId}"`,
  },
  {
    action: "kill_switch",
    label: "Kill Switch",
    group: "destructive",
    tone: "danger",
    requiresExtraCheckbox: true,
    consequence: "Halts the session, cancels open orders, refuses to retry.",
    effect: "Last-resort emergency stop. Cannot be undone.",
    confirmation: (botId) => `KILL "${botId}"`,
  },
];

export function capabilityForAction(
  capabilities: TradingCapabilities,
  action: LiveControlAction,
): TradingControlCapability {
  if (action === "redeem_all") return capabilities.redeem;
  return capabilities[action];
}

export function fingerprintPrefix(session: { config_fingerprint: string } | null): string {
  return (session?.config_fingerprint ?? "").slice(0, 12);
}

export interface CashHeadroom {
  inPlay: number;
  cap: number;
  fraction: number;
  tone: "neutral" | "warning" | "danger";
  available: number;
}

export function cashHeadroom(account: RealAccountSummary): CashHeadroom | null {
  if (account.cash_cap_usd == null || account.cash_cap_usd <= 0) {
    return null;
  }
  const reserved = account.reserved_cash ?? 0;
  const inventory = account.inventory_mark_value ?? 0;
  const pendingRedeem = account.pending_redeem_value ?? 0;
  const inPlay = Math.max(0, reserved + inventory + pendingRedeem);
  const cap = account.cash_cap_usd;
  const fraction = cap > 0 ? Math.min(1, inPlay / cap) : 0;
  const tone = fraction >= 0.85 ? "danger" : fraction >= 0.7 ? "warning" : "neutral";
  return { inPlay, cap, fraction, tone, available: Math.max(0, cap - inPlay) };
}

export interface FreshnessSignal {
  stale: boolean;
  userStreamStale: boolean;
  refreshStale: boolean;
  userStreamStatus: string | null;
}

export function freshnessSignal(
  account: RealAccountSummary,
  now: number = Date.now(),
  thresholds: { userStreamMs: number; refreshMs: number } = { userStreamMs: 60_000, refreshMs: 30_000 },
): FreshnessSignal {
  const userStreamStatus = account.user_stream_status ?? null;
  const userStreamUnhealthy = userStreamStatus != null && userStreamStatus !== "ok";
  const userStreamAge =
    account.last_user_stream_event_at_ms != null
      ? now - account.last_user_stream_event_at_ms
      : null;
  const refreshAge =
    account.last_account_refresh_at_ms != null
      ? now - account.last_account_refresh_at_ms
      : null;
  const userStreamStale =
    userStreamUnhealthy || (userStreamAge != null && userStreamAge > thresholds.userStreamMs);
  const refreshStale = refreshAge != null && refreshAge > thresholds.refreshMs;
  return {
    stale: userStreamStale || refreshStale,
    userStreamStale,
    refreshStale,
    userStreamStatus,
  };
}

export interface ParsedAuditDetails {
  reason: string | null;
  status: string | null;
  command: string | null;
}

export function parseAuditDetails(detailsJson: string | null | undefined): ParsedAuditDetails {
  if (!detailsJson) {
    return { reason: null, status: null, command: null };
  }
  try {
    const parsed = JSON.parse(detailsJson) as Record<string, unknown>;
    const pickString = (value: unknown): string | null =>
      typeof value === "string" && value.length > 0 ? value : null;
    return {
      reason: pickString(parsed.reason),
      status: pickString(parsed.status),
      command: pickString(parsed.command),
    };
  } catch {
    return { reason: null, status: null, command: null };
  }
}
