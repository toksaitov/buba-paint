import type { TradeRow } from "../../lib/types";
import { formatTime, formatUsd, cn } from "../../lib/utils";

export function OpenTrades({ trades }: { trades: TradeRow[] }) {
  const open = trades.filter((t) => t.status === "open");

  if (open.length === 0) {
    return (
      <div className="border border-border px-3 py-2.5 bg-bg">
        <div className="text-[10px] uppercase tracking-wide text-muted mb-1.5">
          Open Trades
        </div>
        <div className="text-[12px] text-muted">No open trades</div>
      </div>
    );
  }

  return (
    <div className="border border-border p-4 bg-bg">
      <div className="text-[11px] uppercase tracking-wide text-muted mb-2">
        Open Trades
      </div>
      <div className="space-y-1.5">
        {open.map((t) => (
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
              <span className="tabular-nums">{formatUsd(t.size)}</span>
              <span className="text-muted tabular-nums">
                {formatTime(t.timestamp)}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
