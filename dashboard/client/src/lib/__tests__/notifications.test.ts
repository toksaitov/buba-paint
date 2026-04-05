import { vi, beforeEach } from "vitest";
import {
  isNotificationSupported,
  isNotificationEnabled,
  setNotificationEnabled,
  requestNotificationPermission,
  showTradeNotification,
} from "../notifications";

const MockNotification = vi.fn();

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();

  Object.defineProperty(globalThis, "Notification", {
    writable: true,
    configurable: true,
    value: Object.assign(MockNotification, {
      permission: "default" as NotificationPermission,
      requestPermission: vi.fn().mockResolvedValue("granted"),
    }),
  });
  MockNotification.mockClear();
});

test("isNotificationSupported returns true when Notification exists", () => {
  expect(isNotificationSupported()).toBe(true);
});

test("isNotificationEnabled reads from localStorage", () => {
  expect(isNotificationEnabled()).toBe(false);
  setNotificationEnabled(true);
  expect(isNotificationEnabled()).toBe(true);
  setNotificationEnabled(false);
  expect(isNotificationEnabled()).toBe(false);
});

test("requestNotificationPermission calls Notification.requestPermission", async () => {
  const result = await requestNotificationPermission();
  expect(Notification.requestPermission).toHaveBeenCalled();
  expect(result).toBe(true);
});

test("requestNotificationPermission returns false when denied", async () => {
  (Notification.requestPermission as ReturnType<typeof vi.fn>).mockResolvedValue("denied");
  const result = await requestNotificationPermission();
  expect(result).toBe(false);
});

test("showTradeNotification creates notification when conditions met", () => {
  Object.assign(Notification, { permission: "granted" });
  setNotificationEnabled(true);
  Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });

  showTradeNotification({ side: "UP", strategy: "latency-arb", pnl: 5.5 });
  expect(MockNotification).toHaveBeenCalledWith(
    "Trade: UP (latency-arb)",
    expect.objectContaining({ body: "PnL: +$5.50" }),
  );
});

test("showTradeNotification skips when page is visible", () => {
  Object.assign(Notification, { permission: "granted" });
  setNotificationEnabled(true);
  Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });

  showTradeNotification({ side: "UP", strategy: "latency-arb" });
  expect(MockNotification).not.toHaveBeenCalled();
});

test("showTradeNotification skips when not enabled", () => {
  Object.assign(Notification, { permission: "granted" });
  setNotificationEnabled(false);
  Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });

  showTradeNotification({ side: "UP", strategy: "latency-arb" });
  expect(MockNotification).not.toHaveBeenCalled();
});
