import { describe, expect, test } from "vitest";
import {
  alertSummaryLabel,
  capabilityEntries,
  healthTone,
  highestAlertTone,
  mobileHeaderStateLabel,
  processStateLabel,
  processStateTone,
  runtimeModeLabel,
  tradingStateLabel,
  tradingStateTone,
} from "../trading-summary";

describe("runtime and trading labels", () => {
  test("formats known runtime modes", () => {
    expect(runtimeModeLabel("paper")).toBe("Paper");
    expect(runtimeModeLabel("live_readonly")).toBe("Live Readonly");
    expect(runtimeModeLabel("live_trading")).toBe("Live Trading");
    expect(runtimeModeLabel("anything-else")).toBe("Paper");
  });

  test("formats known trading states", () => {
    expect(tradingStateLabel("paper")).toBe("Paper");
    expect(tradingStateLabel("readonly")).toBe("Read only");
    expect(tradingStateLabel("gated")).toBe("Gated");
    expect(tradingStateLabel("degraded")).toBe("Degraded");
    expect(tradingStateLabel("disarmed")).toBe("Disarmed");
    expect(tradingStateLabel("armed")).toBe("Armed");
    expect(tradingStateLabel("halted")).toBe("Halted");
    expect(tradingStateLabel("unknown_order")).toBe("Unknown order");
    expect(tradingStateLabel("unknown")).toBe("Paper");
  });

  test("formats process labels and tones", () => {
    expect(processStateLabel("running")).toBe("Running");
    expect(processStateLabel("monitoring")).toBe("Monitor Only");
    expect(processStateLabel("stopped")).toBe("Stopped");
    expect(processStateTone("running")).toBe("success");
    expect(processStateTone("monitoring")).toBe("warning");
    expect(processStateTone("stopped")).toBe("danger");
  });
});

describe("health, alerts, and capabilities", () => {
  test("maps health states to tones", () => {
    expect(healthTone({ state: "healthy", label: "", detail: null })).toBe("success");
    expect(healthTone({ state: "critical", label: "", detail: null })).toBe("danger");
    expect(healthTone({ state: "warning", label: "", detail: null })).toBe("warning");
    expect(healthTone({ state: "idle", label: "", detail: null })).toBe("muted");
  });

  test("returns capability entries in control order", () => {
    const entries = capabilityEntries({
      preflight: { enabled: false, reason: "p" },
      arm: { enabled: false, reason: "a" },
      disarm: { enabled: false, reason: "d" },
      cancel_all: { enabled: false, reason: "c" },
      stop_after_flat: { enabled: false, reason: "s" },
      redeem: { enabled: false, reason: "r" },
      kill_switch: { enabled: false, reason: "k" },
    });
    expect(entries.map((entry) => entry.key)).toEqual([
      "preflight",
      "arm",
      "disarm",
      "cancel_all",
      "stop_after_flat",
      "redeem",
      "kill_switch",
    ]);
  });

  test("summarizes alerts by severity", () => {
    expect(highestAlertTone([])).toBe("muted");
    expect(
      highestAlertTone([{ severity: "warning", title: "warn", detail: "detail" }]),
    ).toBe("warning");
    expect(
      highestAlertTone([{ severity: "critical", title: "crit", detail: "detail" }]),
    ).toBe("danger");
  });

  test("builds alert summary labels", () => {
    expect(
      alertSummaryLabel({
        alerts: [],
      } as never),
    ).toBe("No Alerts");
    expect(
      alertSummaryLabel({
        alerts: [{ severity: "warning", title: "warn", detail: "detail" }],
      } as never),
    ).toBe("1 Notice");
    expect(
      alertSummaryLabel({
        alerts: [{ severity: "critical", title: "crit", detail: "detail" }],
      } as never),
    ).toBe("1 Alert");
    expect(
      alertSummaryLabel({
        alerts: [
          { severity: "critical", title: "crit-1", detail: "detail" },
          { severity: "warning", title: "warn-1", detail: "detail" },
        ],
      } as never),
    ).toBe("2 Alerts");
  });

  test("maps trading states to tones", () => {
    expect(tradingStateTone("readonly")).toBe("neutral");
    expect(tradingStateTone("degraded")).toBe("warning");
    expect(tradingStateTone("armed")).toBe("danger");
    expect(tradingStateTone("halted")).toBe("danger");
    expect(tradingStateTone("unknown_order")).toBe("danger");
    expect(tradingStateTone("gated")).toBe("muted");
    expect(tradingStateTone("paper")).toBe("muted");
  });
});

describe("mobileHeaderStateLabel", () => {
  test("returns Paper for paper runtime", () => {
    expect(mobileHeaderStateLabel("paper", "paper")).toBe("Paper");
  });

  test("combines live runtime with trading state", () => {
    expect(mobileHeaderStateLabel("live_readonly", "readonly")).toBe("Readonly");
    expect(mobileHeaderStateLabel("live_trading", "armed")).toBe("Armed");
    expect(mobileHeaderStateLabel("live_readonly", "degraded")).toBe("Degraded");
  });
});
