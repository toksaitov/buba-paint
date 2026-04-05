import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  ArrowLeftRight,
  LineChart,
  Radio,
  ScrollText,
  BarChart3,
  Bot as BotIcon,
} from "lucide-react";
import { cn } from "../../lib/utils";
import type { Bot } from "../../lib/types";

const pages = [
  { to: "/", icon: LayoutDashboard, label: "Overview" },
  { to: "/equity", icon: LineChart, label: "Equity" },
  { to: "/trades", icon: ArrowLeftRight, label: "Trades" },
  { to: "/signals", icon: Radio, label: "Signals" },
  { to: "/logs", icon: ScrollText, label: "Logs" },
  { to: "/stats", icon: BarChart3, label: "Stats" },
];

interface NavProps {
  collapsed: boolean;
  bots: Bot[];
  activeBotId: string;
  onSelectBot: (id: string) => void;
  onNavigate?: () => void;
}

export function Nav({ collapsed, bots, activeBotId, onSelectBot, onNavigate }: NavProps) {
  return (
    <nav className="flex flex-col flex-1 overflow-y-auto pb-[env(safe-area-inset-bottom)]">
      {bots.length > 0 && (
        <div className="pt-2 pb-0.5">
          {!collapsed && (
            <div className="text-[9px] uppercase tracking-widest text-muted px-3 mb-0.5">
              Bot
            </div>
          )}
          {bots.map((b) =>
            collapsed ? (
              <button
                key={b.id}
                onClick={() => onSelectBot(b.id)}
                title={b.name}
                className={cn(
                  "w-full flex justify-center py-1 transition-colors",
                  b.id === activeBotId
                    ? "bg-text text-bg"
                    : "text-muted hover:bg-surface",
                )}
              >
                <BotIcon size={14} strokeWidth={2} />
              </button>
            ) : (
              <button
                key={b.id}
                onClick={() => onSelectBot(b.id)}
                className={cn(
                  "w-full text-left px-3 py-2 md:py-1 text-[13px] md:text-[12px] transition-colors truncate",
                  b.id === activeBotId
                    ? "bg-text text-bg font-semibold"
                    : "text-muted hover:bg-surface",
                )}
              >
                {b.name}
              </button>
            ),
          )}
        </div>
      )}
      <div className="pt-1.5 pb-2 flex flex-col">
        {!collapsed && (
          <div className="text-[9px] uppercase tracking-widest text-muted px-3 mb-0.5">
            Pages
          </div>
        )}
        {pages.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            onClick={onNavigate}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 px-3 py-2.5 md:py-1 text-[14px] md:text-[12px] transition-colors",
                isActive
                  ? "bg-text text-bg font-semibold"
                  : "text-text hover:bg-surface",
                collapsed && "justify-center",
              )
            }
          >
            <Icon size={14} strokeWidth={2} />
            {!collapsed && <span>{label}</span>}
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
