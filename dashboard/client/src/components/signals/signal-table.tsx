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

export function SignalTable({ signals }: { signals: SignalRow[] }) {
  return (
    <div className="border border-border bg-bg overflow-hidden">
      <div className="overflow-x-auto">
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
    </div>
  );
}

