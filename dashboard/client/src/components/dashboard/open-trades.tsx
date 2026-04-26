import { useEffect, useState } from "react";
import type { TradeRow } from "../../lib/types";
import { empty } from "../../lib/copy";
import { formatDurationShort, formatUsd, cn } from "../../lib/utils";

export function OpenTrades({ trades }: { trades: TradeRow[] }) {
  const open = trades.filter((t) => t.status === "open");
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  if (open.length === 0) {
    return <div className="text-[12px] text-muted">{empty.noOpenTrades}</div>;
  }

  return (
    <div>
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
                Opened {formatDurationShort(Math.max(0, now - t.timestamp))} ago
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
