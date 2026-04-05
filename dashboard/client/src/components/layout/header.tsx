import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  LogOut,
  PanelLeftClose,
  PanelLeft,
  Play,
  Square,
  RotateCw,
  X,
  Bell,
  BellOff,
  Sun,
  Moon,
  Monitor,
} from "lucide-react";
import { useAuth } from "../../hooks/use-auth";
import { useBotStatus } from "../../hooks/use-bot-status";
import { useProcessStatus } from "../../hooks/use-process-status";
import { useTheme } from "../../hooks/use-theme";
import { botStart, botStop, botRestart } from "../../lib/api";
import { cn } from "../../lib/utils";
import {
  isNotificationSupported,
  isNotificationEnabled,
  setNotificationEnabled,
  requestNotificationPermission,
} from "../../lib/notifications";
import type { Bot } from "../../lib/types";

interface HeaderProps {
  bot: Bot | null;
  botId: string;
  collapsed: boolean;
  onToggle: () => void;
  isDesktop: boolean;
}

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h}h ${m}m`;
}

export function Header({ bot, botId, collapsed, onToggle, isDesktop }: HeaderProps) {
  const { user, logout } = useAuth();
  const { data: process } = useProcessStatus(botId);
  const { data: status } = useBotStatus(botId);
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notifyEnabled, setNotifyEnabled] = useState(isNotificationEnabled);
  const { mode: themeMode, setMode: setThemeMode } = useTheme();

  const cycleTheme = () => {
    const next = themeMode === "system" ? "dark" : themeMode === "dark" ? "light" : "system";
    setThemeMode(next);
  };

  const ThemeIcon = themeMode === "dark" ? Moon : themeMode === "light" ? Sun : Monitor;
  const themeTitle = themeMode === "system" ? "Theme: system" : themeMode === "dark" ? "Theme: dark" : "Theme: light";

  const toggleNotifications = async () => {
    if (!isNotificationSupported()) return;
    if (!notifyEnabled) {
      const granted = await requestNotificationPermission();
      if (granted) {
        setNotificationEnabled(true);
        setNotifyEnabled(true);
      }
    } else {
      setNotificationEnabled(false);
      setNotifyEnabled(false);
    }
  };

  const processRunning = process?.active ?? false;
  const controlAvailable = process?.control_available ?? true;
  const observedRunning =
    !processRunning &&
    !controlAvailable &&
    status?.last_tick_at != null &&
    Date.now() - status.last_tick_at < 15_000;
  const isRunning = processRunning || observedRunning;
  const actionUnavailableTitle = controlAvailable
    ? null
    : "Process control unavailable in monitor-only mode";

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({
        queryKey: ["process-status", botId],
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Action failed";
      setError(msg);
      setTimeout(() => setError(null), 6000);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <header className="flex items-center justify-between h-12 px-2 md:px-4 border-b border-border bg-bg shrink-0 pt-[env(safe-area-inset-top)] pr-[env(safe-area-inset-right)]">
        <div className="flex items-center gap-1.5 md:gap-3 min-w-0 overflow-hidden">
          <button
            onClick={onToggle}
            className="p-1 hover:bg-surface transition-colors shrink-0"
            title={isDesktop ? (collapsed ? "Expand sidebar" : "Collapse sidebar") : "Expand navigation"}
            aria-label={isDesktop ? (collapsed ? "Expand sidebar" : "Collapse sidebar") : "Expand navigation"}
          >
            {isDesktop ? (
              collapsed ? <PanelLeft size={16} /> : <PanelLeftClose size={16} />
            ) : (
              <PanelLeft size={16} />
            )}
          </button>
          {process && (
            <span
              className={cn(
                "inline-flex items-center gap-1 text-[11px] font-medium px-1.5 py-0.5 border shrink-0",
                isRunning
                  ? "border-accent-green text-accent-green"
                  : "border-accent-red text-accent-red",
              )}
            >
              <span
                className={cn(
                  "w-1.5 h-1.5 rounded-full shrink-0",
                  isRunning ? "bg-accent-green" : "bg-accent-red",
                )}
              />
              {isRunning ? "Running" : "Stopped"}
            </span>
          )}
          {process?.uptime_secs != null && isRunning && (
            <span className="text-[11px] text-muted hidden md:inline shrink-0">
              {formatUptime(process.uptime_secs)}
            </span>
          )}
          <span className="text-[12px] md:text-[13px] font-bold tracking-tight truncate text-muted md:text-text">
            {bot?.name ?? "buba-paint"}
          </span>
        </div>
        <div className="flex items-center gap-0.5 shrink-0">
          {botId && (
            <div className="flex items-center gap-0.5 mr-1 md:mr-3">
              <button
                onClick={() => run(() => botStart(botId))}
                disabled={busy || !controlAvailable || isRunning}
                className="p-1 md:p-1.5 hover:bg-surface transition-colors text-accent-green disabled:opacity-40"
                title={
                  actionUnavailableTitle ??
                  (isRunning ? "Bot is already running" : "Start bot")
                }
              >
                <Play size={14} />
              </button>
              <button
                onClick={() => run(() => botStop(botId))}
                disabled={busy || !controlAvailable || !processRunning}
                className="p-1 md:p-1.5 hover:bg-surface transition-colors text-accent-red disabled:opacity-40"
                title={
                  actionUnavailableTitle ??
                  (!processRunning ? "Bot is not running" : "Stop bot")
                }
              >
                <Square size={14} />
              </button>
              <button
                onClick={() => run(() => botRestart(botId))}
                disabled={busy || !controlAvailable || !processRunning}
                className={cn(
                  "p-1 md:p-1.5 hover:bg-surface transition-colors text-muted disabled:opacity-40",
                  busy && "animate-spin",
                )}
                title={
                  actionUnavailableTitle ??
                  (!processRunning ? "Bot is not running" : "Restart bot")
                }
              >
                <RotateCw size={14} />
              </button>
            </div>
          )}
          <div className="flex items-center gap-1 md:gap-3 text-[12px] text-muted">
            {user && <span className="hidden md:inline">{user.username}</span>}
            <button
              onClick={cycleTheme}
              className="p-1 hover:bg-surface transition-colors"
              title={themeTitle}
            >
              <ThemeIcon size={14} />
            </button>
            {isNotificationSupported() && (
              <button
                onClick={toggleNotifications}
                className="p-1 hover:bg-surface transition-colors"
                title={notifyEnabled ? "Disable notifications" : "Enable notifications"}
              >
                {notifyEnabled ? <Bell size={14} /> : <BellOff size={14} />}
              </button>
            )}
            <button
              onClick={logout}
              className="p-1 hover:bg-surface transition-colors"
              title="Logout"
            >
              <LogOut size={14} />
            </button>
          </div>
        </div>
      </header>
      {error && (
        <div className="flex items-center gap-2 px-3 md:px-4 py-2 bg-accent-red/10 border-b border-accent-red/30 text-accent-red text-[12px]">
          <span className="flex-1">{error}</span>
          <button
            onClick={() => setError(null)}
            className="p-0.5 hover:bg-accent-red/20 transition-colors shrink-0"
          >
            <X size={12} />
          </button>
        </div>
      )}
    </>
  );
}
