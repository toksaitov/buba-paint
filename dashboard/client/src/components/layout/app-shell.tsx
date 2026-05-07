import { useState, useEffect } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Nav } from "./nav";
import { Header } from "./header";
import { Logo } from "./logo";
import { ContextStrip } from "../ui/dashboard-primitives";
import { getBots } from "../../lib/api";
import { useArmedSync } from "../../hooks/use-armed-sync";
import { useLiveUpdates } from "../../hooks/use-live-updates";
import { useMediaQuery } from "../../hooks/use-media-query";
import { useTradingSummary } from "../../hooks/use-trading-summary";
import { routeMetaForPath } from "../../lib/routes";
import type { DashboardRouteMeta } from "../../lib/routes";
import type { TradingSummary } from "../../lib/types";
import { useMobileNavStore } from "../../stores/mobile-nav-store";

function contextDescription(routeMeta: DashboardRouteMeta, summary?: TradingSummary) {
  const mode = summary?.runtime_mode ?? "paper";
  if (routeMeta.to === "/") {
    return mode === "paper"
      ? "Simulated performance. Polymarket account shows only in live mode."
      : "Simulated performance and a quick look at the Polymarket account. Open Execution for venue detail.";
  }
  if (routeMeta.to === "/execution") {
    return mode === "paper"
      ? "Simulated execution. Nothing touches Polymarket."
      : "Live venue state. Switch to Overview for simulated results.";
  }
  return routeMeta.contextDescription;
}

export function AppShell() {
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const location = useLocation();
  const [collapsed, setCollapsed] = useState(false);
  const drawerOpen = useMobileNavStore((s) => s.isOpen);
  const closeDrawer = useMobileNavStore((s) => s.close);
  const toggleDrawer = useMobileNavStore((s) => s.toggle);

  const [selectedBotId, setSelectedBotId] = useState<string | null>(
    sessionStorage.getItem("activeBotId"),
  );

  const { data } = useQuery({
    queryKey: ["bots"],
    queryFn: getBots,
  });

  const bots = data?.bots ?? [];

  useEffect(() => {
    if (bots.length > 0) {
      if (!selectedBotId || !bots.find((b) => b.id === selectedBotId)) {
        setSelectedBotId(bots[0].id);
      }
    }
  }, [bots, selectedBotId]);

  const bot = bots.find((b) => b.id === selectedBotId) ?? null;
  const botId = bot?.id ?? "";
  const routeMeta = routeMetaForPath(location.pathname);
  const { data: tradingSummary } = useTradingSummary(botId);

  useLiveUpdates(botId);
  useArmedSync(botId);

  useEffect(() => {
    if (botId) sessionStorage.setItem("activeBotId", botId);
  }, [botId]);

  useEffect(() => {
    if (isDesktop) closeDrawer();
  }, [isDesktop, closeDrawer]);

  return (
    <div className="flex h-[100dvh] min-h-[100svh] overflow-hidden">
      {isDesktop ? (
        <aside
          className={`${collapsed ? "w-12" : "w-52"} border-r border-border bg-bg shrink-0 transition-all duration-150 flex flex-col`}
        >
          <div
            className={`flex items-center h-14 border-b border-border ${collapsed ? "justify-center" : "px-3"}`}
          >
            <Link
              to="/"
              aria-label="Go to main page"
              className={`flex items-center ${collapsed ? "justify-center" : ""} hover:opacity-80 transition-opacity`}
            >
              <Logo size={collapsed ? 20 : 18} />
              {!collapsed && (
                <span className="ml-2 text-xl font-semibold tracking-tight">
                  buba
                </span>
              )}
            </Link>
          </div>
          <Nav
            collapsed={collapsed}
            bots={bots}
            activeBotId={botId}
            onSelectBot={setSelectedBotId}
          />
        </aside>
      ) : (
        <>
          <aside className="w-12 border-r border-border bg-bg shrink-0 flex flex-col pt-[var(--app-safe-top)] pl-[var(--app-safe-left)]">
            <div className="flex items-center justify-center h-14 border-b border-border">
              <Link to="/" aria-label="Go to main page" className="hover:opacity-80 transition-opacity">
                <Logo size={20} />
              </Link>
            </div>
            <Nav
              collapsed={true}
              bots={bots}
              activeBotId={botId}
              onSelectBot={setSelectedBotId}
            />
          </aside>
          {drawerOpen && (
            <div className="fixed inset-0 z-50 flex">
              <div
                className="absolute inset-0 bg-black/40"
                onClick={closeDrawer}
                aria-hidden="true"
              />
              <aside className="relative z-10 w-64 bg-bg border-r border-border flex flex-col pt-[var(--app-safe-top)] pl-[var(--app-safe-left)]">
                <div className="flex items-center h-14 border-b border-border px-3">
                  <Link
                    to="/"
                    onClick={closeDrawer}
                    aria-label="Go to main page"
                    className="flex items-center hover:opacity-80 transition-opacity"
                  >
                    <Logo size={18} />
                    <span className="ml-2 text-lg font-semibold tracking-tight">
                      buba
                    </span>
                  </Link>
                </div>
                <Nav
                  collapsed={false}
                  bots={bots}
                  activeBotId={botId}
                  onSelectBot={(id) => {
                    setSelectedBotId(id);
                    closeDrawer();
                  }}
                  onNavigate={closeDrawer}
                />
              </aside>
            </div>
          )}
        </>
      )}

      <div className="flex flex-col flex-1 min-w-0">
        <Header
          bot={bot}
          botId={botId}
          collapsed={collapsed}
          onToggle={isDesktop ? () => setCollapsed((c) => !c) : toggleDrawer}
          isDesktop={isDesktop}
        />
        {routeMeta.showContextStrip && (
          <ContextStrip
            title={routeMeta.contextTitle}
            description={contextDescription(routeMeta, tradingSummary)}
          />
        )}
        <main className="flex-1 overflow-y-auto p-3 pr-[max(0.75rem,var(--app-safe-right))] pb-[max(0.75rem,var(--app-safe-bottom))]">
          <Outlet context={{ botId, bot }} />
        </main>
      </div>
    </div>
  );
}
