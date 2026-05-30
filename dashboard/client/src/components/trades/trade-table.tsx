import { useEffect, useMemo, useState } from "react";
import type { TradeRow } from "../../lib/types";
import { cn, formatDateTime, formatSignedUsd, formatUsd, pnlColor } from "../../lib/utils";
import { TableToolbar } from "../ui/dashboard-primitives";

interface TradeTableProps {
  trades: TradeRow[];
  page: number;
  total: number;
  perPage: number;
  onPageChange: (page: number) => void;
}

interface TradeFilterPreferences {
  strategy: string;
  side: string;
  status: string;
  market: string;
}

const TRADE_FILTERS_STORAGE_KEY = "buba.trades.filters.v1";
const DEFAULT_TRADE_FILTERS: TradeFilterPreferences = {
  strategy: "all",
  side: "all",
  status: "all",
  market: "",
};

function isTradeSide(value: unknown): value is TradeFilterPreferences["side"] {
  return value === "all" || value === "UP" || value === "DOWN";
}

function readTradeFilterPreferences(): TradeFilterPreferences {
  try {
    const raw = localStorage.getItem(TRADE_FILTERS_STORAGE_KEY);
    if (!raw) return DEFAULT_TRADE_FILTERS;
    const parsed = JSON.parse(raw) as Partial<TradeFilterPreferences>;
    return {
      strategy: typeof parsed.strategy === "string" ? parsed.strategy : DEFAULT_TRADE_FILTERS.strategy,
      side: isTradeSide(parsed.side) ? parsed.side : DEFAULT_TRADE_FILTERS.side,
      status: typeof parsed.status === "string" ? parsed.status : DEFAULT_TRADE_FILTERS.status,
      market: typeof parsed.market === "string" ? parsed.market : DEFAULT_TRADE_FILTERS.market,
    };
  } catch {
    return DEFAULT_TRADE_FILTERS;
  }
}

function sideTone(side: string) {
  return side === "UP" ? "text-accent-green border-accent-green" : "text-accent-red border-accent-red";
}

function settleValue(trade: TradeRow) {
  return trade.settlement_price != null ? trade.settlement_price.toFixed(1) : "--";
}

function pnlValue(trade: TradeRow) {
  return trade.pnl != null ? formatSignedUsd(trade.pnl) : "--";
}

function MobileTradeCard({ trade }: { trade: TradeRow }) {
  return (
    <div className="border-b border-surface px-3 py-3 last:border-0">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", sideTone(trade.side))}>
              {trade.side}
            </span>
            <span className="text-[12px] text-text">{trade.strategy}</span>
          </div>
          <div className="mt-1 text-[11px] text-muted tabular-nums">
            {formatDateTime(trade.timestamp)}
          </div>
        </div>
        <div className={cn("text-[12px] font-semibold tabular-nums", pnlColor(trade.pnl ?? 0))}>
          {pnlValue(trade)}
        </div>
      </div>
      <div className="mt-3 grid grid-cols-3 gap-2 text-[11px]">
        <div>
          <div className="text-muted">Size</div>
          <div className="mt-1 tabular-nums">{formatUsd(trade.size)}</div>
        </div>
        <div>
          <div className="text-muted">Entry</div>
          <div className="mt-1 tabular-nums">{trade.entry_price.toFixed(4)}</div>
        </div>
        <div>
          <div className="text-muted">Settle</div>
          <div className="mt-1 tabular-nums">{settleValue(trade)}</div>
        </div>
      </div>
    </div>
  );
}

export function TradeTable({
  trades,
  page,
  total,
  perPage,
  onPageChange,
}: TradeTableProps) {
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const [filters, setFilters] = useState(readTradeFilterPreferences);
  const strategies = useMemo(
    () => Array.from(new Set(trades.map((trade) => trade.strategy))).sort(),
    [trades],
  );
  const statuses = useMemo(
    () => Array.from(new Set(trades.map((trade) => trade.status))).sort(),
    [trades],
  );
  const effectiveFilters = useMemo(
    () => ({
      ...filters,
      strategy:
        filters.strategy === "all" || strategies.includes(filters.strategy)
          ? filters.strategy
          : "all",
      status:
        filters.status === "all" || statuses.includes(filters.status)
          ? filters.status
          : "all",
    }),
    [filters, statuses, strategies],
  );
  const { strategy, side, status, market } = effectiveFilters;
  const filtered = useMemo(
    () =>
      trades.filter((trade) => {
        if (strategy !== "all" && trade.strategy !== strategy) return false;
        if (side !== "all" && trade.side !== side) return false;
        if (status !== "all" && trade.status !== status) return false;
        if (market.trim() && !trade.market_id.toLowerCase().includes(market.trim().toLowerCase())) {
          return false;
        }
        return true;
      }),
    [market, side, status, strategy, trades],
  );
  const allFilteredSameStatus =
    filtered.length > 0 && new Set(filtered.map((trade) => trade.status)).size === 1;

  useEffect(() => {
    try {
      localStorage.setItem(
        TRADE_FILTERS_STORAGE_KEY,
        JSON.stringify(effectiveFilters),
      );
    } catch {
      return;
    }
  }, [effectiveFilters]);

  return (
    <div className="border border-border bg-bg">
      <TableToolbar
        left={
          <div className="text-[11px] text-muted">
            {filtered.length === trades.length
              ? `${trades.length} trades`
              : `${filtered.length} of ${trades.length} trades`}
          </div>
        }
        right={
          <>
            <select
              aria-label="Strategy"
              value={strategy}
              onChange={(event) =>
                setFilters((current) => ({
                  ...current,
                  strategy: event.target.value,
                  status,
                }))
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value="all">All strategies</option>
              {strategies.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <select
              aria-label="Side"
              value={side}
              onChange={(event) =>
                setFilters((current) => ({
                  ...current,
                  strategy,
                  side: event.target.value,
                  status,
                }))
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value="all">All sides</option>
              <option value="UP">UP</option>
              <option value="DOWN">DOWN</option>
            </select>
            <select
              aria-label="Status"
              value={status}
              onChange={(event) =>
                setFilters((current) => ({
                  ...current,
                  strategy,
                  status: event.target.value,
                }))
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value="all">All statuses</option>
              {statuses.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <input
              value={market}
              onChange={(event) =>
                setFilters((current) => ({
                  ...current,
                  strategy,
                  status,
                  market: event.target.value,
                }))
              }
              placeholder="Market"
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            />
          </>
        }
      />
      <div className="hidden overflow-x-auto md:block">
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-border bg-surface">
              <th className="px-3 py-2 text-left font-semibold text-muted">Time</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Strategy</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Side</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">Size</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">Entry</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">Settle</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">PnL</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((trade) => (
              <tr key={trade.id} className="border-b border-surface last:border-0">
                <td className="px-3 py-2 text-muted tabular-nums">{formatDateTime(trade.timestamp)}</td>
                <td className="px-3 py-2">
                  <div className="font-medium">{trade.strategy}</div>
                  {!allFilteredSameStatus && (
                    <div className="text-[11px] text-muted">{trade.status}</div>
                  )}
                </td>
                <td className="px-3 py-2">
                  <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", sideTone(trade.side))}>
                    {trade.side}
                  </span>
                </td>
                <td className="px-3 py-2 text-right tabular-nums">{formatUsd(trade.size)}</td>
                <td className="px-3 py-2 text-right tabular-nums">{trade.entry_price.toFixed(4)}</td>
                <td className="px-3 py-2 text-right tabular-nums text-muted">{settleValue(trade)}</td>
                <td className={cn("px-3 py-2 text-right font-semibold tabular-nums", pnlColor(trade.pnl ?? 0))}>
                  {pnlValue(trade)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="md:hidden">
        {filtered.map((trade) => (
          <MobileTradeCard key={trade.id} trade={trade} />
        ))}
      </div>

      {totalPages > 1 && (
        <div className="flex items-center justify-between border-t border-border px-3 py-2 text-[11px]">
          <span className="text-muted">
            {total} trades, page {page}/{totalPages}
          </span>
          <div className="flex gap-1">
            <button
              onClick={() => onPageChange(page - 1)}
              disabled={page <= 1}
              className="border border-border px-2 py-1 transition-colors hover:bg-surface disabled:opacity-30"
            >
              Prev
            </button>
            <button
              onClick={() => onPageChange(page + 1)}
              disabled={page >= totalPages}
              className="border border-border px-2 py-1 transition-colors hover:bg-surface disabled:opacity-30"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
