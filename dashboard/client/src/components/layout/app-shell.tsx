import { useState, useEffect } from "react";
import { Link, Outlet } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Nav } from "./nav";
import { Header } from "./header";
import { Logo } from "./logo";
import { getBots } from "../../lib/api";
import { useLiveUpdates } from "../../hooks/use-live-updates";

export function AppShell() {
  const [collapsed, setCollapsed] = useState(false);
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

  return (
    <div className="flex h-screen overflow-hidden">
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
      <div className="flex flex-col flex-1 min-w-0">
        <Header
          bot={bot}
          botId={botId}
          collapsed={collapsed}
          onToggle={() => setCollapsed((c) => !c)}
        />
        <main className="flex-1 overflow-y-auto p-3">
          <Outlet context={{ botId, bot }} />
        </main>
      </div>
    </div>
  );
}
