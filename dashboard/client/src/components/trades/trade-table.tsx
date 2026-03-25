import type { TradeRow } from "../../lib/types";
import { formatDateTime, formatUsd, pnlColor, cn } from "../../lib/utils";

interface TradeTableProps {
  trades: TradeRow[];
  page: number;
  total: number;
  perPage: number;
  onPageChange: (p: number) => void;
}

export function TradeTable({
  trades,
  page,
  total,
  perPage,
  onPageChange,
}: TradeTableProps) {
  const totalPages = Math.max(1, Math.ceil(total / perPage));

  return (
    <div className="border border-border bg-bg overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-border bg-surface">
              <th className="text-left px-3 py-2 font-semibold">Time</th>
              <th className="text-left px-3 py-2 font-semibold">Strategy</th>
              <th className="text-left px-3 py-2 font-semibold">Side</th>
              <th className="text-right px-3 py-2 font-semibold">Size</th>
              <th className="text-right px-3 py-2 font-semibold">Entry</th>
              <th className="text-right px-3 py-2 font-semibold">Settle</th>
              <th className="text-right px-3 py-2 font-semibold">PnL</th>
            </tr>
          </thead>
          <tbody>
            {trades.map((t) => (
              <tr key={t.id} className="border-b border-surface last:border-0">
                <td className="px-3 py-1.5 tabular-nums text-muted">
                  {formatDateTime(t.timestamp)}
                </td>
                <td className="px-3 py-1.5">{t.strategy}</td>
                <td className="px-3 py-1.5">
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
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums">
                  {formatUsd(t.size)}
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums">
                  {t.entry_price.toFixed(4)}
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums text-muted">
                  {t.settlement_price !== null
                    ? t.settlement_price.toFixed(1)
                    : "--"}
                </td>
                <td
                  className={cn(
                    "px-3 py-1.5 text-right tabular-nums font-medium",
                    pnlColor(t.pnl ?? 0),
                  )}
                >
                  {t.pnl !== null
                    ? `${t.pnl >= 0 ? "+" : ""}${formatUsd(t.pnl)}`
                    : "--"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {totalPages > 1 && (
        <div className="flex items-center justify-between px-3 py-2 border-t border-border text-[11px]">
          <span className="text-muted">
            {total} trades, page {page}/{totalPages}
          </span>
          <div className="flex gap-1">
            <button
              onClick={() => onPageChange(page - 1)}
              disabled={page <= 1}
              className="px-2 py-0.5 border border-border rounded disabled:opacity-30 hover:bg-surface transition-colors"
            >
              Prev
            </button>
            <button
              onClick={() => onPageChange(page + 1)}
              disabled={page >= totalPages}
              className="px-2 py-0.5 border border-border rounded disabled:opacity-30 hover:bg-surface transition-colors"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
