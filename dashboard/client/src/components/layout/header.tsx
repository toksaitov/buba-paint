import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  Bell,
  BellOff,
  LogOut,
  Monitor,
  Moon,
  MoreHorizontal,
  PanelLeft,
  PanelLeftClose,
  Play,
  RotateCw,
  Square,
  Sun,
} from "lucide-react";
import { useAuth } from "../../hooks/use-auth";
import { useProcessStatus } from "../../hooks/use-process-status";
import { useTheme } from "../../hooks/use-theme";
import { useTradingSummary } from "../../hooks/use-trading-summary";
import { botRestart, botStart, botStop } from "../../lib/api";
import {
  isNotificationEnabled,
  isNotificationSupported,
  requestNotificationPermission,
  setNotificationEnabled,
} from "../../lib/notifications";
import {
  type ChipTone,
  mobileHeaderStateLabel,
  processStateLabel,
  processStateTone,
  runtimeModeLabel,
  tradingStateLabel,
  tradingStateTone,
} from "../../lib/trading-summary";
import { cn } from "../../lib/utils";
import type { Bot } from "../../lib/types";
import { StatusChip } from "../ui/dashboard-primitives";

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
  const hours = Math.floor(secs / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

function visibleHeaderState(
  runtimeMode: string,
  tradingState: string,
): Array<{ label: string; tone: ChipTone }> {
  if (runtimeMode === "paper" && tradingState === "paper") {
    return [];
  }

  if (runtimeMode === "live_readonly" && tradingState === "readonly") {
    return [{ label: runtimeModeLabel(runtimeMode), tone: "muted" }];
  }

  const badges: Array<{ label: string; tone: ChipTone }> = [
    { label: runtimeModeLabel(runtimeMode), tone: "muted" },
  ];
  if (tradingState !== "paper") {
    badges.push({
      label: tradingStateLabel(tradingState),
      tone: tradingStateTone(tradingState),
    });
  }
  return badges;
}

export function Header({
  bot,
  botId,
  collapsed,
  onToggle,
  isDesktop,
}: HeaderProps) {
  const { user, logout } = useAuth();
  const { data: process } = useProcessStatus(botId);
  const { data: tradingSummary } = useTradingSummary(botId);
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notifyEnabled, setNotifyEnabled] = useState(isNotificationEnabled);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const { mode: themeMode, setMode: setThemeMode, theme } = useTheme();
  const armed = theme === "armed";
  const effectiveMobileMenuOpen = !isDesktop && mobileMenuOpen;

  const cycleTheme = () => {
    const next =
      themeMode === "system" ? "dark" : themeMode === "dark" ? "light" : "system";
    setThemeMode(next);
  };

  const toggleNotifications = async () => {
    if (!isNotificationSupported()) return;
    if (!notifyEnabled) {
      const granted = await requestNotificationPermission();
      if (granted) {
        setNotificationEnabled(true);
        setNotifyEnabled(true);
      }
      return;
    }
    setNotificationEnabled(false);
    setNotifyEnabled(false);
  };

  const ThemeIcon =
    themeMode === "dark" ? Moon : themeMode === "light" ? Sun : Monitor;
  const themeLabel =
    themeMode === "system"
      ? "Theme: system"
      : themeMode === "dark"
        ? "Theme: dark"
        : "Theme: light";
  const processRunning = process?.active ?? tradingSummary?.process_state === "running";
  const controlAvailable = process?.control_available ?? true;
  const runtimeMode = tradingSummary?.runtime_mode ?? "paper";
  const tradingState = tradingSummary?.trading_state ?? "paper";
  const stateBadges = visibleHeaderState(runtimeMode, tradingState);
  const actionUnavailableTitle = controlAvailable
    ? null
    : "Process control unavailable in monitor-only mode";

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["process-status", botId] }),
        queryClient.invalidateQueries({ queryKey: ["trading-summary", botId] }),
      ]);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Action failed";
      setError(message);
    } finally {
      setBusy(false);
    }
  };

  const runMobile = (action: () => Promise<unknown>) => {
    setMobileMenuOpen(false);
    void run(action);
  };

  const controlButtonClass =
    "inline-flex items-center justify-center min-h-[44px] min-w-[44px] lg:min-h-0 lg:min-w-0 p-2 lg:p-2.5 transition-colors hover:bg-surface disabled:opacity-40";
  const mobileMenuButtonClass =
    "flex min-h-[44px] w-full items-center gap-3 border-b border-border px-3 py-2 text-left text-[12px] last:border-b-0 hover:bg-surface disabled:opacity-40";

  return (
    <>
      <header className="app-header relative flex shrink-0 items-end justify-between border-b border-border bg-bg px-2 pb-0 pr-[max(0.5rem,var(--app-safe-right))] md:items-center md:px-4 md:pr-4">
        <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden md:gap-3">
          <button
            onClick={onToggle}
            className="shrink-0 inline-flex items-center justify-center min-h-[44px] min-w-[44px] lg:min-h-0 lg:min-w-0 p-2 lg:p-2.5 transition-colors hover:bg-surface"
            title={
              isDesktop
                ? collapsed
                  ? "Expand sidebar"
                  : "Collapse sidebar"
                : "Expand navigation"
            }
            aria-label={
              isDesktop
                ? collapsed
                  ? "Expand sidebar"
                  : "Collapse sidebar"
                : "Expand navigation"
            }
          >
            {isDesktop ? (
              collapsed ? <PanelLeft size={16} /> : <PanelLeftClose size={16} />
            ) : (
              <PanelLeft size={16} />
            )}
          </button>
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden md:gap-3">
            <div className="flex shrink-0 items-center gap-1.5">
              <StatusChip
                label={processStateLabel(tradingSummary?.process_state ?? "stopped")}
                tone={processStateTone(tradingSummary?.process_state ?? "stopped")}
                dot={processRunning}
                compact
                title={
                  process?.uptime_secs != null && processRunning
                    ? `Process uptime ${formatUptime(process.uptime_secs)}`
                    : "Current bot process state"
                }
              />
              {!isDesktop && (
                <StatusChip
                  label={mobileHeaderStateLabel(runtimeMode, tradingState)}
                  tone={runtimeMode === "paper" ? "muted" : tradingStateTone(tradingState)}
                  compact
                />
              )}
            </div>
            <div className="min-w-0">
              <div className="truncate text-[14px] font-semibold tracking-tight">
                {bot?.name ?? "buba"}
              </div>
              <div className="flex items-center">
                {bot && <div className="truncate text-[11px] text-muted">{bot.id}</div>}
              </div>
            </div>
            {isDesktop && (
              <div className="hidden flex-wrap items-center gap-1.5 md:flex">
                {stateBadges.map((badge) => (
                  <StatusChip key={badge.label} label={badge.label} tone={badge.tone} compact />
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="hidden shrink-0 items-center gap-1 md:flex lg:gap-0.5">
          {botId && (
            <div className="mr-2 flex items-center gap-1 lg:gap-0.5 md:mr-3">
              <button
                onClick={() => run(() => botStart(botId))}
                disabled={busy || !controlAvailable || processRunning}
                className={cn(controlButtonClass, "text-accent-green")}
                title={
                  actionUnavailableTitle ??
                  (processRunning ? "Bot is already running" : "Start bot")
                }
                aria-label={
                  actionUnavailableTitle ??
                  (processRunning ? "Bot is already running" : "Start bot")
                }
              >
                <Play size={14} />
              </button>
              <button
                onClick={() => run(() => botStop(botId))}
                disabled={busy || !controlAvailable || !process?.active}
                className={cn(controlButtonClass, "text-accent-red")}
                title={
                  actionUnavailableTitle ??
                  (!process?.active ? "Bot is not running" : "Stop bot")
                }
                aria-label={
                  actionUnavailableTitle ??
                  (!process?.active ? "Bot is not running" : "Stop bot")
                }
              >
                <Square size={14} />
              </button>
              <button
                onClick={() => run(() => botRestart(botId))}
                disabled={busy || !controlAvailable || !process?.active}
                className={cn(
                  controlButtonClass,
                  "text-muted",
                  busy && "animate-spin",
                )}
                title={
                  actionUnavailableTitle ??
                  (!process?.active ? "Bot is not running" : "Restart bot")
                }
                aria-label={
                  actionUnavailableTitle ??
                  (!process?.active ? "Bot is not running" : "Restart bot")
                }
              >
                <RotateCw size={14} />
              </button>
            </div>
          )}

          <div className="flex items-center gap-1 text-[12px] text-muted lg:gap-0.5 md:gap-3">
            {user && <span className="hidden md:inline">{user.username}</span>}
            {!armed && (
              <button
                onClick={cycleTheme}
                className={controlButtonClass}
                title={themeLabel}
                aria-label={themeLabel}
              >
                <ThemeIcon size={14} />
              </button>
            )}
            {isNotificationSupported() && (
              <button
                onClick={toggleNotifications}
                className={controlButtonClass}
                title={notifyEnabled ? "Disable notifications" : "Enable notifications"}
                aria-label={notifyEnabled ? "Disable notifications" : "Enable notifications"}
              >
                {notifyEnabled ? <Bell size={14} /> : <BellOff size={14} />}
              </button>
            )}
            <button
              onClick={logout}
              className={controlButtonClass}
              title="Logout"
              aria-label="Logout"
            >
              <LogOut size={14} />
            </button>
          </div>
        </div>
        <div className="flex shrink-0 items-center md:hidden">
          <button
            type="button"
            onClick={() => setMobileMenuOpen((open) => !open)}
            className="inline-flex min-h-[44px] min-w-[44px] items-center justify-center p-2 transition-colors hover:bg-surface"
            title="More header controls"
            aria-label="More header controls"
            aria-expanded={effectiveMobileMenuOpen}
            aria-controls="mobile-header-controls"
          >
            <MoreHorizontal size={16} />
          </button>
          {effectiveMobileMenuOpen && (
            <div
              id="mobile-header-controls"
              role="menu"
              className="app-header-menu absolute right-[max(0.5rem,var(--app-safe-right))] z-40 w-56 border border-border bg-bg shadow-none"
            >
              {botId && (
                <>
                  <button
                    type="button"
                    onClick={() => runMobile(() => botStart(botId))}
                    disabled={busy || !controlAvailable || processRunning}
                    className={cn(mobileMenuButtonClass, "text-accent-green")}
                    title={
                      actionUnavailableTitle ??
                      (processRunning ? "Bot is already running" : "Start bot")
                    }
                    aria-label={
                      actionUnavailableTitle ??
                      (processRunning ? "Bot is already running" : "Start bot")
                    }
                  >
                    <Play size={14} />
                    <span>Start bot</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => runMobile(() => botStop(botId))}
                    disabled={busy || !controlAvailable || !process?.active}
                    className={cn(mobileMenuButtonClass, "text-accent-red")}
                    title={
                      actionUnavailableTitle ??
                      (!process?.active ? "Bot is not running" : "Stop bot")
                    }
                    aria-label={
                      actionUnavailableTitle ??
                      (!process?.active ? "Bot is not running" : "Stop bot")
                    }
                  >
                    <Square size={14} />
                    <span>Stop bot</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => runMobile(() => botRestart(botId))}
                    disabled={busy || !controlAvailable || !process?.active}
                    className={cn(mobileMenuButtonClass, "text-muted", busy && "animate-spin")}
                    title={
                      actionUnavailableTitle ??
                      (!process?.active ? "Bot is not running" : "Restart bot")
                    }
                    aria-label={
                      actionUnavailableTitle ??
                      (!process?.active ? "Bot is not running" : "Restart bot")
                    }
                  >
                    <RotateCw size={14} />
                    <span>Restart bot</span>
                  </button>
                </>
              )}
              {!armed && (
                <button
                  type="button"
                  onClick={() => {
                    cycleTheme();
                    setMobileMenuOpen(false);
                  }}
                  className={mobileMenuButtonClass}
                  title={themeLabel}
                  aria-label={themeLabel}
                >
                  <ThemeIcon size={14} />
                  <span>{themeLabel}</span>
                </button>
              )}
              {isNotificationSupported() && (
                <button
                  type="button"
                  onClick={() => {
                    setMobileMenuOpen(false);
                    void toggleNotifications();
                  }}
                  className={mobileMenuButtonClass}
                  title={notifyEnabled ? "Disable notifications" : "Enable notifications"}
                  aria-label={notifyEnabled ? "Disable notifications" : "Enable notifications"}
                >
                  {notifyEnabled ? <Bell size={14} /> : <BellOff size={14} />}
                  <span>{notifyEnabled ? "Disable notifications" : "Enable notifications"}</span>
                </button>
              )}
              <button
                type="button"
                onClick={() => {
                  setMobileMenuOpen(false);
                  logout();
                }}
                className={mobileMenuButtonClass}
                title="Logout"
                aria-label="Logout"
              >
                <LogOut size={14} />
                <span>Logout</span>
              </button>
            </div>
          )}
        </div>
      </header>
      {error && (
        <div
          role="alert"
          aria-live="polite"
          className="flex items-start justify-between gap-3 border-b border-border bg-bg px-3 py-2 text-[11px] text-accent-red"
        >
          <span className="min-w-0">{error}</span>
          <button
            type="button"
            onClick={() => setError(null)}
            className="shrink-0 px-2 py-0.5 text-[11px] text-muted hover:bg-surface"
            aria-label="Dismiss error"
          >
            Dismiss
          </button>
        </div>
      )}
    </>
  );
}
