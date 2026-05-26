import { Bot as BotIcon } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { cn } from "../../lib/utils";
import { dashboardRoutes } from "../../lib/routes";
import type { Bot } from "../../lib/types";

function pickActiveNavTarget(pathname: string): string | null {
  const exact = dashboardRoutes.find((route) => route.to === pathname);
  if (exact) return exact.to;
  const prefixed = dashboardRoutes
    .filter(
      (route) => route.to !== "/" && pathname.startsWith(`${route.to}/`),
    )
    .sort((a, b) => b.to.length - a.to.length);
  return prefixed[0]?.to ?? null;
}

interface NavProps {
  collapsed: boolean;
  bots: Bot[];
  activeBotId: string;
  onSelectBot: (id: string) => void;
  onNavigate?: () => void;
}

const groupedRoutes = [
  {
    label: "Monitor",
    routes: dashboardRoutes.filter((route) => route.section === "Monitor"),
  },
  {
    label: "Analysis",
    routes: dashboardRoutes.filter((route) => route.section === "Analysis"),
  },
  {
    label: "Research",
    routes: dashboardRoutes.filter((route) => route.section === "Research"),
  },
];

export function Nav({
  collapsed,
  bots,
  activeBotId,
  onSelectBot,
  onNavigate,
}: NavProps) {
  const location = useLocation();
  const activeTarget = pickActiveNavTarget(location.pathname);
  return (
    <nav className="flex flex-col flex-1 overflow-y-auto pb-[var(--app-safe-bottom)]">
      {bots.length > 0 && (
        <div className="pt-2 pb-1" role="radiogroup" aria-label="Select bot">
          {!collapsed && (
            <div className="px-3 text-[10px] text-muted">Bot</div>
          )}
          {bots.map((bot) =>
            collapsed ? (
              <button
                key={bot.id}
                onClick={() => onSelectBot(bot.id)}
                role="radio"
                aria-checked={bot.id === activeBotId}
                title={bot.name}
                aria-label={bot.name}
                className={cn(
                  "flex w-full justify-center py-2 transition-colors",
                  bot.id === activeBotId
                    ? "bg-text text-bg"
                    : "text-muted hover:bg-surface",
                )}
              >
                <BotIcon size={16} strokeWidth={2} />
              </button>
            ) : (
              <button
                key={bot.id}
                onClick={() => onSelectBot(bot.id)}
                role="radio"
                aria-checked={bot.id === activeBotId}
                title={bot.name}
                className={cn(
                  "w-full truncate px-3 py-2.5 text-left text-[13px] transition-colors md:py-2",
                  bot.id === activeBotId
                    ? "bg-text font-semibold text-bg"
                    : "text-muted hover:bg-surface",
                )}
              >
                {bot.name}
              </button>
            ),
          )}
        </div>
      )}
      <div className="pb-2">
        {groupedRoutes.map((group) => (
          <div key={group.label} className="pt-2">
            {!collapsed && (
              <div className="px-3 text-[10px] text-muted">{group.label}</div>
            )}
            {group.routes.map(({ to, icon: Icon, label }) => {
              const isActive = activeTarget === to;
              return (
                <Link
                  key={to}
                  to={to}
                  onClick={onNavigate}
                  aria-current={isActive ? "page" : undefined}
                  className={cn(
                    "flex items-center gap-2 px-3 py-3 text-[14px] transition-colors md:py-2 md:text-[13px]",
                    isActive
                      ? "bg-text font-semibold text-bg"
                      : "text-text hover:bg-surface",
                    collapsed && "justify-center",
                  )}
                >
                  <Icon size={16} strokeWidth={2} />
                  {!collapsed && <span>{label}</span>}
                </Link>
              );
            })}
          </div>
        ))}
      </div>
    </nav>
  );
}
