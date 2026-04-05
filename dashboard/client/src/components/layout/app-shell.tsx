import { useState, useEffect } from "react";
import { Link, Outlet } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Nav } from "./nav";
import { Header } from "./header";
import { Logo } from "./logo";
import { getBots } from "../../lib/api";
import { useLiveUpdates } from "../../hooks/use-live-updates";
import { useMediaQuery } from "../../hooks/use-media-query";
import { useMobileNavStore } from "../../stores/mobile-nav-store";

export function AppShell() {
  const isDesktop = useMediaQuery("(min-width: 768px)");
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

  useLiveUpdates(botId);

  useEffect(() => {
    if (botId) sessionStorage.setItem("activeBotId", botId);
  }, [botId]);

  useEffect(() => {
    if (isDesktop) closeDrawer();
  }, [isDesktop, closeDrawer]);

  return (
    <div className="flex h-[100dvh] overflow-hidden">
      {isDesktop ? (
        <aside
          className={`${collapsed ? "w-12" : "w-52"} border-r border-border bg-bg shrink-0 transition-all duration-150 flex flex-col`}
        >
          <div
            className={`flex items-center h-12 border-b border-border ${collapsed ? "justify-center" : "px-3"}`}
          >
            <Link
              to="/"
              aria-label="Go to main page"
              className={`flex items-center ${collapsed ? "justify-center" : ""} hover:opacity-80 transition-opacity`}
            >
              <Logo size={collapsed ? 20 : 18} />
              {!collapsed && (
                <span className="text-[11px] font-bold uppercase tracking-widest text-muted ml-2">
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
          <aside className="w-12 border-r border-border bg-bg shrink-0 flex flex-col pt-[env(safe-area-inset-top)] pl-[env(safe-area-inset-left)]">
            <div className="flex items-center justify-center h-12 border-b border-border">
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
              <aside className="relative z-10 w-64 bg-bg border-r border-border flex flex-col pt-[env(safe-area-inset-top)] pl-[env(safe-area-inset-left)]">
                <div className="flex items-center h-12 border-b border-border px-3">
                  <Link
                    to="/"
                    onClick={closeDrawer}
                    aria-label="Go to main page"
                    className="flex items-center hover:opacity-80 transition-opacity"
                  >
                    <Logo size={18} />
                    <span className="text-[11px] font-bold uppercase tracking-widest text-muted ml-2">
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
        <main className="flex-1 overflow-y-auto p-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">
          <Outlet context={{ botId, bot }} />
        </main>
      </div>
    </div>
  );
}
