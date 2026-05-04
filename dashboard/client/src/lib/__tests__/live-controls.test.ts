import { describe, expect, test } from "vitest";
import {
  capabilityForAction,
  cashHeadroom,
  controlActions,
  fingerprintPrefix,
  freshnessSignal,
  parseAuditDetails,
  visibleControlActions,
} from "../live-controls";
import type { RealAccountSummary, TradingCapabilities } from "../types";

const baseAccount: RealAccountSummary = {
  available_cash: 750,
  reserved_cash: 0,
  inventory_mark_value: 0,
  redeemable_value: 0,
  pending_redeem_value: 0,
  total_equity: 750,
  allowance_available: 750,
  latest_snapshot_at_ms: 1000,
  session_id: 1,
  session_status: "armed",
  session_started_at_ms: 0,
  wallet_address: "0xwallet",
  proxy_wallet: "0xproxy",
  cash_cap_usd: 1000,
  enabled_strategies: [],
  provider: "polymarket",
  user_stream_status: "ok",
  last_user_stream_connected_at_ms: 1000,
  last_user_stream_event_at_ms: 1000,
  last_account_refresh_at_ms: 1000,
  open_orders: 0,
  pending_redemptions: 0,
  critical_reconciliation_events: 0,
};

const baseCapabilities: TradingCapabilities = {
  preflight: { enabled: true, reason: "ready" },
  arm: { enabled: true, reason: "gates green" },
  disarm: { enabled: false, reason: "not armed" },
  cancel_all: { enabled: false, reason: "no open" },
  stop_after_flat: { enabled: false, reason: "not armed" },
  redeem: { enabled: false, reason: "no redeemable" },
  kill_switch: { enabled: true, reason: "halt available" },
};

describe("controlActions", () => {
  test("declares stable group taxonomy", () => {
    const groups = Object.fromEntries(controlActions.map((c) => [c.action, c.group]));
    expect(groups.preflight).toBe("dryrun");
    expect(groups.arm).toBe("reversible");
    expect(groups.disarm).toBe("reversible");
    expect(groups.stop_after_flat).toBe("reversible");
    expect(groups.cancel_all).toBe("destructive");
    expect(groups.redeem_all).toBe("destructive");
    expect(groups.kill_switch).toBe("destructive");
  });

  test("kill_switch requires extra checkbox", () => {
    const kill = controlActions.find((c) => c.action === "kill_switch");
    expect(kill?.requiresExtraCheckbox).toBe(true);
  });

  test("arm and redeem_all are desktop-only", () => {
    const arm = controlActions.find((c) => c.action === "arm");
    const redeem = controlActions.find((c) => c.action === "redeem_all");
    expect(arm?.desktopOnly).toBe(true);
    expect(redeem?.desktopOnly).toBe(true);
  });

  test("renders confirmation phrases for destructive actions with quoted bot id", () => {
    const arm = controlActions.find((c) => c.action === "arm");
    const cancel = controlActions.find((c) => c.action === "cancel_all");
    const redeem = controlActions.find((c) => c.action === "redeem_all");
    const kill = controlActions.find((c) => c.action === "kill_switch");
    expect(arm?.confirmation?.("paint", "fingerprint12")).toBe('ARM "paint" fingerprint12');
    expect(cancel?.confirmation?.("paint", "")).toBe('CANCEL ALL "paint"');
    expect(redeem?.confirmation?.("paint", "")).toBe('REDEEM ALL "paint"');
    expect(kill?.confirmation?.("paint", "")).toBe('KILL "paint"');
  });
});

describe("visibleControlActions", () => {
  test("disarmed limits to preflight and arm", () => {
    const actions = visibleControlActions("disarmed").map((c) => c.action);
    expect(actions).toEqual(["preflight", "arm"]);
  });

  test("armed exposes the destructive set plus disarm and stop after flat", () => {
    const actions = visibleControlActions("armed").map((c) => c.action);
    expect(actions).toEqual(["disarm", "stop_after_flat", "cancel_all", "redeem_all", "kill_switch"]);
  });

  test("stop_after_flat keeps recovery actions but not arm", () => {
    const actions = visibleControlActions("stop_after_flat").map((c) => c.action);
    expect(actions).toContain("disarm");
    expect(actions).toContain("kill_switch");
    expect(actions).not.toContain("arm");
  });

  test("unknown_order narrows to reconciliation actions", () => {
    const actions = visibleControlActions("unknown_order").map((c) => c.action);
    expect(actions).toEqual(["preflight", "cancel_all", "kill_switch"]);
  });

  test("halted only exposes recovery actions", () => {
    const actions = visibleControlActions("halted").map((c) => c.action);
    expect(actions).toEqual(["cancel_all", "redeem_all"]);
  });

  test("default exposes preflight and kill switch only", () => {
    const actions = visibleControlActions("readonly").map((c) => c.action);
    expect(actions).toEqual(["preflight", "kill_switch"]);
  });
});

describe("capabilityForAction", () => {
  test("maps redeem_all to capabilities.redeem", () => {
    expect(capabilityForAction(baseCapabilities, "redeem_all")).toBe(baseCapabilities.redeem);
  });

  test("indexes by action otherwise", () => {
    expect(capabilityForAction(baseCapabilities, "preflight")).toBe(baseCapabilities.preflight);
    expect(capabilityForAction(baseCapabilities, "kill_switch")).toBe(baseCapabilities.kill_switch);
  });
});

describe("fingerprintPrefix", () => {
  test("returns 12-char prefix", () => {
    expect(fingerprintPrefix({ config_fingerprint: "abcdef0123456789zzz" })).toBe("abcdef012345");
  });

  test("returns empty when no session", () => {
    expect(fingerprintPrefix(null)).toBe("");
  });
});

describe("cashHeadroom", () => {
  test("returns null when no cap is set", () => {
    expect(cashHeadroom({ ...baseAccount, cash_cap_usd: null })).toBeNull();
  });

  test("computes neutral tone below 70%", () => {
    const head = cashHeadroom({
      ...baseAccount,
      reserved_cash: 100,
      inventory_mark_value: 200,
      pending_redeem_value: 0,
    });
    expect(head?.inPlay).toBe(300);
    expect(head?.cap).toBe(1000);
    expect(head?.fraction).toBeCloseTo(0.3, 5);
    expect(head?.tone).toBe("neutral");
    expect(head?.available).toBe(700);
  });

  test("warning tone between 70 and 85%", () => {
    const head = cashHeadroom({
      ...baseAccount,
      reserved_cash: 800,
      inventory_mark_value: 0,
      pending_redeem_value: 0,
    });
    expect(head?.tone).toBe("warning");
  });

  test("danger tone at or above 85%", () => {
    const head = cashHeadroom({
      ...baseAccount,
      reserved_cash: 900,
      inventory_mark_value: 0,
      pending_redeem_value: 0,
    });
    expect(head?.tone).toBe("danger");
  });

  test("clamps to 1.0 when in-play exceeds cap", () => {
    const head = cashHeadroom({
      ...baseAccount,
      reserved_cash: 1500,
      inventory_mark_value: 0,
      pending_redeem_value: 0,
    });
    expect(head?.fraction).toBe(1);
    expect(head?.available).toBe(0);
  });
});

describe("freshnessSignal", () => {
  test("flags ok stream and recent refresh as fresh", () => {
    const result = freshnessSignal(baseAccount, baseAccount.last_user_stream_event_at_ms!);
    expect(result.stale).toBe(false);
    expect(result.userStreamStale).toBe(false);
    expect(result.refreshStale).toBe(false);
  });

  test("flags non-ok stream as stale immediately", () => {
    const result = freshnessSignal(
      { ...baseAccount, user_stream_status: "stalled" },
      baseAccount.last_user_stream_event_at_ms!,
    );
    expect(result.stale).toBe(true);
    expect(result.userStreamStale).toBe(true);
  });

  test("flags stale user stream by age", () => {
    const result = freshnessSignal(baseAccount, baseAccount.last_user_stream_event_at_ms! + 90_000);
    expect(result.userStreamStale).toBe(true);
    expect(result.stale).toBe(true);
  });

  test("flags stale account refresh by age", () => {
    const result = freshnessSignal(
      {
        ...baseAccount,
        last_user_stream_event_at_ms: baseAccount.last_user_stream_event_at_ms,
        last_account_refresh_at_ms: baseAccount.last_account_refresh_at_ms,
      },
      baseAccount.last_account_refresh_at_ms! + 60_000,
    );
    expect(result.refreshStale).toBe(true);
  });
});

describe("parseAuditDetails", () => {
  test("returns nulls for missing JSON", () => {
    expect(parseAuditDetails(null)).toEqual({ reason: null, status: null, command: null });
    expect(parseAuditDetails("")).toEqual({ reason: null, status: null, command: null });
  });

  test("returns nulls for invalid JSON", () => {
    expect(parseAuditDetails("not-json")).toEqual({ reason: null, status: null, command: null });
  });

  test("extracts known fields", () => {
    const json = JSON.stringify({
      reason: "manual gate refresh",
      status: "applied",
      command: "preflight",
      extra: "ignored",
    });
    expect(parseAuditDetails(json)).toEqual({
      reason: "manual gate refresh",
      status: "applied",
      command: "preflight",
    });
  });

  test("ignores non-string fields", () => {
    const json = JSON.stringify({ reason: 42, status: null });
    expect(parseAuditDetails(json)).toEqual({ reason: null, status: null, command: null });
  });
});
