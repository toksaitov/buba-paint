import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RotateCw } from "lucide-react";
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
import { cn } from "../../lib/utils";

const PULL_REFRESH_THRESHOLD_PX = 72;
const PULL_REFRESH_MAX_PX = 104;
const PULL_REFRESH_START_PX = 8;

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
  const queryClient = useQueryClient();
  const mainScrollRef = useRef<HTMLElement | null>(null);
  const pullStartYRef = useRef<number | null>(null);
  const pullStartXRef = useRef<number | null>(null);
  const pullTrackingRef = useRef(false);
  const pullDistanceRef = useRef(0);
  const pullRefreshingRef = useRef(false);
  const [collapsed, setCollapsed] = useState(false);
  const [pullDistance, setPullDistanceState] = useState(0);
  const [pullRefreshing, setPullRefreshing] = useState(false);
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

  const bots = useMemo(() => data?.bots ?? [], [data?.bots]);
  const activeBotId =
    selectedBotId && bots.some((b) => b.id === selectedBotId)
      ? selectedBotId
      : bots[0]?.id;
  const bot = bots.find((b) => b.id === activeBotId) ?? null;
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

  useEffect(() => {
    pullRefreshingRef.current = pullRefreshing;
  }, [pullRefreshing]);

  const setPullDistance = useCallback((distance: number) => {
    pullDistanceRef.current = distance;
    setPullDistanceState(distance);
  }, []);

  const resetPullGesture = useCallback(() => {
    pullStartYRef.current = null;
    pullStartXRef.current = null;
    pullTrackingRef.current = false;
    setPullDistance(0);
  }, [setPullDistance]);

  const refreshDashboard = useCallback(async () => {
    setPullRefreshing(true);
    try {
      await queryClient.invalidateQueries();
      await queryClient.refetchQueries({ type: "active" });
    } finally {
      setPullRefreshing(false);
      resetPullGesture();
    }
  }, [queryClient, resetPullGesture]);

  const beginPull = useCallback(
    (clientX: number, clientY: number, touchCount: number) => {
      const main = mainScrollRef.current;
      if (pullRefreshingRef.current || touchCount !== 1 || (main?.scrollTop ?? 0) > 0) {
        resetPullGesture();
        return;
      }
      pullStartYRef.current = clientY;
      pullStartXRef.current = clientX;
      pullTrackingRef.current = false;
    },
    [resetPullGesture],
  );

  const movePull = useCallback(
    (clientX: number, clientY: number, preventDefault: () => void) => {
      const main = mainScrollRef.current;
      const startY = pullStartYRef.current;
      const startX = pullStartXRef.current;
      if (startY == null || startX == null || pullRefreshingRef.current) return;

      const deltaY = clientY - startY;
      const deltaX = clientX - startX;
      if (deltaY <= 0) {
        setPullDistance(0);
        return;
      }
      if ((main?.scrollTop ?? 0) > 0) {
        resetPullGesture();
        return;
      }
      if (!pullTrackingRef.current) {
        if (Math.abs(deltaX) > deltaY) {
          resetPullGesture();
          return;
        }
        if (deltaY < PULL_REFRESH_START_PX) return;
        pullTrackingRef.current = true;
      }

      const resistedDistance = Math.min(PULL_REFRESH_MAX_PX, Math.sqrt(deltaY) * 8.5);
      setPullDistance(resistedDistance);
      preventDefault();
    },
    [resetPullGesture, setPullDistance],
  );

  const endPull = useCallback(() => {
    const shouldRefresh = pullDistanceRef.current >= PULL_REFRESH_THRESHOLD_PX;
    pullStartYRef.current = null;
    pullStartXRef.current = null;
    pullTrackingRef.current = false;
    if (shouldRefresh) {
      void refreshDashboard();
      return;
    }
    setPullDistance(0);
  }, [refreshDashboard, setPullDistance]);

  useEffect(() => {
    const main = mainScrollRef.current;
    if (!main) return;

    const handleTouchStart = (event: TouchEvent) => {
      const touch = event.touches[0];
      if (!touch) return;
      beginPull(touch.clientX, touch.clientY, event.touches.length);
    };

    const handleTouchMove = (event: TouchEvent) => {
      const touch = event.touches[0];
      if (!touch) return;
      movePull(touch.clientX, touch.clientY, () => {
        if (event.cancelable) event.preventDefault();
      });
    };

    main.addEventListener("touchstart", handleTouchStart, { passive: true });
    main.addEventListener("touchmove", handleTouchMove, { passive: false });
    main.addEventListener("touchend", endPull, { passive: true });
    main.addEventListener("touchcancel", resetPullGesture, { passive: true });
    return () => {
      main.removeEventListener("touchstart", handleTouchStart);
      main.removeEventListener("touchmove", handleTouchMove);
      main.removeEventListener("touchend", endPull);
      main.removeEventListener("touchcancel", resetPullGesture);
    };
  }, [beginPull, endPull, movePull, resetPullGesture]);

  const pullVisible = pullDistance > 0 || pullRefreshing;
  const pullReady = pullDistance >= PULL_REFRESH_THRESHOLD_PX;

  return (
    <div className="flex h-[100dvh] min-h-[100svh] overflow-hidden">
      {isDesktop ? (
        <aside
          className={`${collapsed ? "w-12" : "w-52"} app-sidebar border-r border-border bg-bg shrink-0 transition-all duration-150 flex flex-col`}
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
          <aside className="app-sidebar w-12 border-r border-border bg-bg shrink-0 flex flex-col pl-[var(--app-safe-left)]">
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
              <aside className="app-sidebar relative z-10 w-64 bg-bg border-r border-border flex flex-col pl-[var(--app-safe-left)]">
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
        <div className="relative min-h-0 flex-1">
          <div
            className={cn(
              "pointer-events-none absolute left-0 right-0 top-0 z-30 flex justify-center transition-[opacity,transform] duration-150",
              pullVisible ? "opacity-100" : "opacity-0",
            )}
            style={{
              transform: `translate3d(0, ${Math.min(pullDistance, PULL_REFRESH_THRESHOLD_PX) - 44}px, 0)`,
            }}
            aria-live="polite"
          >
            <div className="inline-flex items-center gap-2 border border-border bg-bg px-3 py-1.5 text-[11px] text-muted">
              <RotateCw
                size={13}
                className={cn(
                  pullRefreshing && "animate-spin",
                  pullReady && !pullRefreshing && "rotate-180",
                )}
              />
              <span>
                {pullRefreshing
                  ? "Refreshing"
                  : pullReady
                    ? "Release to refresh"
                    : "Pull to refresh"}
              </span>
            </div>
          </div>
          <main
            ref={mainScrollRef}
            data-testid="app-main-scroll"
            className="app-main-scroll h-full overflow-y-auto p-3 pr-[max(0.75rem,var(--app-safe-right))] pb-[max(0.75rem,var(--app-safe-bottom))]"
            style={{
              transform: `translate3d(0, ${pullRefreshing ? 18 : Math.min(pullDistance * 0.24, 22)}px, 0)`,
              transition: pullDistance > 0 || pullRefreshing ? "none" : "transform 160ms ease-out",
            }}
          >
            <Outlet context={{ botId, bot }} />
          </main>
        </div>
      </div>
    </div>
  );
}
