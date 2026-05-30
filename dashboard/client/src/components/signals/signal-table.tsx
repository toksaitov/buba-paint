import { useEffect, useMemo, useState } from "react";
import type { SignalGroupRow, SignalRow } from "../../lib/types";
import { TableToolbar } from "../ui/dashboard-primitives";
import { empty } from "../../lib/copy";
import { cn, formatDateTime } from "../../lib/utils";

function parseMomentum(metadata: string | null) {
  if (!metadata) return null;
  try {
    const parsed = JSON.parse(metadata) as Record<string, unknown>;
    return typeof parsed.momentum === "number" ? parsed.momentum : null;
  } catch {
    return null;
  }
}

function directionTone(direction: string) {
  return direction === "UP" ? "text-accent-green border-accent-green" : "text-accent-red border-accent-red";
}

interface SignalGroupFilterPreferences {
  strategy: string;
  direction: string;
}

const SIGNAL_GROUP_FILTERS_STORAGE_KEY = "buba.signals.groupFilters.v1";
const DEFAULT_SIGNAL_GROUP_FILTERS: SignalGroupFilterPreferences = {
  strategy: "all",
  direction: "all",
};

function isSignalDirection(value: unknown): value is SignalGroupFilterPreferences["direction"] {
  return value === "all" || value === "UP" || value === "DOWN";
}

function readSignalGroupFilterPreferences(): SignalGroupFilterPreferences {
  try {
    const raw = localStorage.getItem(SIGNAL_GROUP_FILTERS_STORAGE_KEY);
    if (!raw) return DEFAULT_SIGNAL_GROUP_FILTERS;
    const parsed = JSON.parse(raw) as Partial<SignalGroupFilterPreferences>;
    return {
      strategy: typeof parsed.strategy === "string" ? parsed.strategy : DEFAULT_SIGNAL_GROUP_FILTERS.strategy,
      direction: isSignalDirection(parsed.direction)
        ? parsed.direction
        : DEFAULT_SIGNAL_GROUP_FILTERS.direction,
    };
  } catch {
    return DEFAULT_SIGNAL_GROUP_FILTERS;
  }
}

function MobileSignalCard({ signal }: { signal: SignalRow }) {
  const momentum = parseMomentum(signal.metadata);

  return (
    <div className="border-b border-surface px-3 py-3 last:border-0">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", directionTone(signal.direction))}>
              {signal.direction}
            </span>
            <span className="text-[12px]">{signal.strategy}</span>
          </div>
          <div className="mt-1 text-[11px] text-muted tabular-nums">
            {formatDateTime(signal.timestamp)}
          </div>
        </div>
        <div className="text-right text-[11px] text-muted tabular-nums">
          {momentum != null ? momentum.toFixed(6) : "--"}
        </div>
      </div>
      <div className="mt-3 grid grid-cols-3 gap-2 text-[11px]">
        <div>
          <div className="text-muted">BTC</div>
          <div className="mt-1 tabular-nums">
            {signal.binance_price != null ? `$${signal.binance_price.toLocaleString()}` : "--"}
          </div>
        </div>
        <div>
          <div className="text-muted">UP Ask</div>
          <div className="mt-1 tabular-nums">
            {signal.up_ask != null ? signal.up_ask.toFixed(4) : "--"}
          </div>
        </div>
        <div>
          <div className="text-muted">DOWN Ask</div>
          <div className="mt-1 tabular-nums">
            {signal.down_ask != null ? signal.down_ask.toFixed(4) : "--"}
          </div>
        </div>
      </div>
    </div>
  );
}

export function SignalTable({ signals }: { signals: SignalRow[] }) {
  return (
    <div className="border border-border bg-bg">
      <div className="hidden overflow-x-auto md:block">
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-border bg-surface">
              <th className="px-3 py-2 text-left font-semibold text-muted">Time</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Strategy</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Direction</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">Momentum</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">BTC (Binance)</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">UP Ask</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">DOWN Ask</th>
            </tr>
          </thead>
          <tbody>
            {signals.map((signal) => {
              const momentum = parseMomentum(signal.metadata);
              return (
                <tr key={signal.id} className="border-b border-surface last:border-0">
                  <td className="px-3 py-2 text-muted tabular-nums">{formatDateTime(signal.timestamp)}</td>
                  <td className="px-3 py-2">
                    <div className="font-medium">{signal.strategy}</div>
                    <div className="text-[11px] text-muted">{signal.market_id ?? empty.noMarketLinked}</div>
                  </td>
                  <td className="px-3 py-2">
                    <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", directionTone(signal.direction))}>
                      {signal.direction}
                    </span>
                  </td>
                  <td className="px-3 py-2 text-right text-muted tabular-nums">
                    {momentum != null ? momentum.toFixed(6) : "--"}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {signal.binance_price != null ? `$${signal.binance_price.toLocaleString()}` : "--"}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {signal.up_ask != null ? signal.up_ask.toFixed(4) : "--"}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {signal.down_ask != null ? signal.down_ask.toFixed(4) : "--"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="md:hidden">
        {signals.map((signal) => (
          <MobileSignalCard key={signal.id} signal={signal} />
        ))}
      </div>
    </div>
  );
}

function groupRange(group: SignalGroupRow) {
  if (group.start_timestamp === group.end_timestamp) {
    return formatDateTime(group.end_timestamp);
  }
  return `${formatDateTime(group.start_timestamp)} to ${formatDateTime(group.end_timestamp)}`;
}

function MobileSignalGroupCard({ group }: { group: SignalGroupRow }) {
  return (
    <div className="border-b border-surface px-3 py-3 last:border-0">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", directionTone(group.direction))}>
              {group.direction}
            </span>
            <span className="text-[12px]">{group.strategy}</span>
          </div>
          <div className="mt-1 text-[11px] text-muted tabular-nums">{groupRange(group)}</div>
        </div>
        <div className="text-right text-[11px] text-muted tabular-nums">
          {group.count} rows
        </div>
      </div>
      <div className="mt-3 grid grid-cols-3 gap-2 text-[11px]">
        <div>
          <div className="text-muted">BTC</div>
          <div className="mt-1 tabular-nums">
            {group.binance_price != null ? `$${group.binance_price.toLocaleString()}` : "--"}
          </div>
        </div>
        <div>
          <div className="text-muted">UP Ask</div>
          <div className="mt-1 tabular-nums">
            {group.up_ask != null ? group.up_ask.toFixed(4) : "--"}
          </div>
        </div>
        <div>
          <div className="text-muted">DOWN Ask</div>
          <div className="mt-1 tabular-nums">
            {group.down_ask != null ? group.down_ask.toFixed(4) : "--"}
          </div>
        </div>
      </div>
    </div>
  );
}

export function SignalGroupTable({ groups }: { groups: SignalGroupRow[] }) {
  const [filters, setFilters] = useState(readSignalGroupFilterPreferences);
  const strategies = useMemo(
    () => Array.from(new Set(groups.map((group) => group.strategy))).sort(),
    [groups],
  );
  const effectiveFilters = useMemo(
    () => ({
      ...filters,
      strategy:
        filters.strategy === "all" || strategies.includes(filters.strategy)
          ? filters.strategy
          : "all",
    }),
    [filters, strategies],
  );
  const { strategy, direction } = effectiveFilters;
  const filtered = useMemo(
    () =>
      groups.filter((group) => {
        if (strategy !== "all" && group.strategy !== strategy) return false;
        if (direction !== "all" && group.direction !== direction) return false;
        return true;
      }),
    [direction, groups, strategy],
  );

  useEffect(() => {
    try {
      localStorage.setItem(
        SIGNAL_GROUP_FILTERS_STORAGE_KEY,
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
            {filtered.length === groups.length
              ? `${groups.length} bursts`
              : `${filtered.length} of ${groups.length} bursts`}
          </div>
        }
        right={
          <>
            <select
              aria-label="Strategy"
              value={strategy}
              onChange={(event) => setFilters((current) => ({ ...current, strategy: event.target.value }))}
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
              aria-label="Direction"
              value={direction}
              onChange={(event) =>
                setFilters((current) => ({
                  ...current,
                  strategy,
                  direction: event.target.value,
                }))
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value="all">All directions</option>
              <option value="UP">UP</option>
              <option value="DOWN">DOWN</option>
            </select>
          </>
        }
      />
      <div className="hidden overflow-x-auto md:block">
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-border bg-surface">
              <th className="px-3 py-2 text-left font-semibold text-muted">Window</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Strategy</th>
              <th className="px-3 py-2 text-left font-semibold text-muted">Direction</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">Rows</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">BTC (Binance)</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">UP Ask</th>
              <th className="px-3 py-2 text-right font-semibold text-muted">DOWN Ask</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((group) => (
              <tr key={group.id} className="border-b border-surface last:border-0">
                <td className="px-3 py-2 text-muted tabular-nums">{groupRange(group)}</td>
                <td className="px-3 py-2">
                  <div className="font-medium">{group.strategy}</div>
                  <div className="text-[11px] text-muted">{group.market_id ?? empty.noMarketLinked}</div>
                </td>
                <td className="px-3 py-2">
                  <span className={cn("border px-1.5 py-0.5 text-[11px] font-semibold", directionTone(group.direction))}>
                    {group.direction}
                  </span>
                </td>
                <td className="px-3 py-2 text-right tabular-nums">{group.count}</td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {group.binance_price != null ? `$${group.binance_price.toLocaleString()}` : "--"}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {group.up_ask != null ? group.up_ask.toFixed(4) : "--"}
                </td>
                <td className="px-3 py-2 text-right tabular-nums">
                  {group.down_ask != null ? group.down_ask.toFixed(4) : "--"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="md:hidden">
        {filtered.map((group) => (
          <MobileSignalGroupCard key={group.id} group={group} />
        ))}
      </div>
    </div>
  );
}
