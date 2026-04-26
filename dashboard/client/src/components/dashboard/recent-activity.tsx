import type { TradeRow } from "../../lib/types";
import { empty } from "../../lib/copy";
import { formatTime, formatUsd, pnlColor, cn } from "../../lib/utils";

export function RecentActivity({ trades }: { trades: TradeRow[] }) {
  const settled = trades
    .filter((t) => t.resolved_at !== null)
    .slice(0, 8);

  return (
    <div>
      {settled.length === 0 ? (
        <div className="text-[12px] text-muted">{empty.noSettledTrades}</div>
      ) : (
        <div className="space-y-1">
          {settled.map((t) => (
            <div
              key={t.id}
              className="flex items-center justify-between text-[12px] py-1 border-b border-surface last:border-0"
            >
              <div className="flex items-center gap-2">
                <span
                  className={cn(
                    "font-semibold",
                    t.side === "UP"
                      ? "text-accent-green"
                      : "text-accent-red",
                  )}
                >
                  {t.side}
                </span>
                <span className="text-muted">{t.strategy}</span>
              </div>
              <div className="flex items-center gap-3">
                <span
                  className={cn("tabular-nums font-medium", pnlColor(t.pnl ?? 0))}
                >
                  {t.pnl !== null
                    ? `${t.pnl >= 0 ? "+" : ""}${formatUsd(t.pnl)}`
                    : "--"}
                </span>
                <span className="text-muted tabular-nums">
                  {formatTime(t.resolved_at!)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
