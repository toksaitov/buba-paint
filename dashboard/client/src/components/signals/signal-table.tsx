import type { SignalRow } from "../../lib/types";
import { formatDateTime, cn } from "../../lib/utils";

function parseMomentum(metadata: string | null): number | null {
  if (!metadata) return null;
  try {
    const parsed = JSON.parse(metadata) as Record<string, unknown>;
    if (typeof parsed.momentum === "number") return parsed.momentum;
  } catch {

  }
  return null;
}

function SignalCard({ signal: s }: { signal: SignalRow }) {
  const momentum = parseMomentum(s.metadata);
  return (
    <div className="flex flex-col gap-0.5 py-2 border-b border-surface last:border-0 px-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "font-semibold text-[12px]",
              s.direction === "UP" ? "text-accent-green" : "text-accent-red",
            )}
          >
            {s.direction}
          </span>
          <span className="text-[12px] text-muted">{s.strategy}</span>
        </div>
        <span className="text-[11px] text-muted tabular-nums">
          {formatDateTime(s.timestamp)}
        </span>
      </div>
      <div className="flex items-center gap-3 text-[12px] tabular-nums">
        {momentum !== null && (
          <span className="text-muted">mom {momentum.toFixed(6)}</span>
        )}
        {s.binance_price !== null && (
          <span>${s.binance_price.toLocaleString()}</span>
        )}
        {s.up_ask !== null && (
          <span className="text-accent-green">{s.up_ask.toFixed(4)}</span>
        )}
        {s.down_ask !== null && (
          <span className="text-accent-red">{s.down_ask.toFixed(4)}</span>
        )}
      </div>
    </div>
  );
}

export function SignalTable({ signals }: { signals: SignalRow[] }) {
  return (
    <div className="border border-border bg-bg overflow-hidden">
      <div className="hidden md:block overflow-x-auto">
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-border bg-surface">
              <th className="text-left px-3 py-2 font-semibold">Time</th>
              <th className="text-left px-3 py-2 font-semibold">Strategy</th>
              <th className="text-left px-3 py-2 font-semibold">Dir</th>
              <th className="text-right px-3 py-2 font-semibold">Momentum</th>
              <th className="text-right px-3 py-2 font-semibold">BTC Price</th>
              <th className="text-right px-3 py-2 font-semibold">UP Ask</th>
              <th className="text-right px-3 py-2 font-semibold">DOWN Ask</th>
            </tr>
          </thead>
          <tbody>
            {signals.map((s) => {
              const momentum = parseMomentum(s.metadata);
              return (
                <tr
                  key={s.id}
                  className="border-b border-surface last:border-0"
                >
                  <td className="px-3 py-1.5 tabular-nums text-muted">
                    {formatDateTime(s.timestamp)}
                  </td>
                  <td className="px-3 py-1.5">{s.strategy}</td>
                  <td className="px-3 py-1.5">
                    <span
                      className={cn(
                        "font-semibold",
                        s.direction === "UP"
                          ? "text-accent-green"
                          : "text-accent-red",
                      )}
                    >
                      {s.direction}
                    </span>
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums text-muted">
                    {momentum !== null ? momentum.toFixed(6) : "--"}
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums">
                    {s.binance_price !== null
                      ? `$${s.binance_price.toLocaleString()}`
                      : "--"}
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums">
                    {s.up_ask !== null ? s.up_ask.toFixed(4) : "--"}
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums">
                    {s.down_ask !== null ? s.down_ask.toFixed(4) : "--"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className="md:hidden">
        {signals.map((s) => (
          <SignalCard key={s.id} signal={s} />
        ))}
      </div>
    </div>
  );
}
