const STORAGE_KEY = "buba-notifications-enabled";

export function isNotificationSupported(): boolean {
  return typeof Notification !== "undefined";
}

export function isNotificationEnabled(): boolean {
  return localStorage.getItem(STORAGE_KEY) === "true";
}

export function setNotificationEnabled(enabled: boolean): void {
  localStorage.setItem(STORAGE_KEY, String(enabled));
}

export async function requestNotificationPermission(): Promise<boolean> {
  if (!isNotificationSupported()) return false;
  const result = await Notification.requestPermission();
  return result === "granted";
}

export function showTradeNotification(data: {
  side?: string;
  strategy?: string;
  pnl?: number | null;
}): void {
  if (!isNotificationSupported()) return;
  if (Notification.permission !== "granted") return;
  if (!isNotificationEnabled()) return;
  if (document.visibilityState !== "hidden") return;

  const title = `Trade: ${data.side ?? "?"} (${data.strategy ?? "unknown"})`;
  const body =
    data.pnl != null
      ? `PnL: ${data.pnl >= 0 ? "+" : ""}$${data.pnl.toFixed(2)}`
      : "New trade opened";

  new Notification(title, { body, icon: "/icon-192x192.png", tag: "buba-trade" });
}
