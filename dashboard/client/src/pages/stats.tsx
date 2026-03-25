import { useOutletContext } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useBotStatus } from "../hooks/use-bot-status";
import { getStats } from "../lib/api";
import { Loading } from "../components/common/loading";
import { formatUsd, pnlColor, cn } from "../lib/utils";
import type { Bot } from "../lib/types";

export function StatsPage() {
  const { botId, bot } = useOutletContext<{ botId: string; bot: Bot | null }>();
  const { data: status } = useBotStatus(botId);
  const { data: stats, isLoading } = useQuery({
    queryKey: ["stats", botId],
    queryFn: () => getStats(botId),
    enabled: !!botId,
  });

  if (isLoading) return <Loading />;

  return (
    <div className="space-y-4">
      <h2 className="text-[14px] font-bold">Bot Status</h2>

      <div className="border border-border bg-bg overflow-hidden">
        <table className="w-full text-[12px]">
          <tbody>
            <Row label="Name" value={bot?.name ?? "--"} />
            <Row label="ID" value={bot?.id ?? "--"} />
            <Row
              label="Balance"
              value={status ? formatUsd(status.balance) : "--"}
            />
            <Row
              label="Uptime"
              value={status ? `${status.uptime_hours.toFixed(1)}h` : "--"}
            />
            <Row
              label="Open Trades"
              value={status?.open_trades?.toString() ?? "0"}
            />
          </tbody>
        </table>
      </div>

      {stats && Object.keys(stats.by_strategy).length > 0 && (
        <>
          <h2 className="text-[14px] font-bold">Strategy Breakdown</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {Object.entries(stats.by_strategy).map(([name, s]) => {
              const avgPnl = s.trades > 0 ? s.total_pnl / s.trades : 0;
              return (
                <div
                  key={name}
                  className="border border-border p-4 bg-bg"
                >
                  <div className="text-[12px] font-bold mb-2">{name}</div>
                  <div className="grid grid-cols-2 gap-y-1 text-[11px]">
                    <span className="text-muted">Trades</span>
                    <span className="text-right tabular-nums">{s.trades}</span>
                    <span className="text-muted">Win Rate</span>
                    <span className="text-right tabular-nums">
                      {s.win_rate.toFixed(1)}%
                    </span>
                    <span className="text-muted">W/L</span>
                    <span className="text-right tabular-nums">
                      {s.wins}/{s.losses}
                    </span>
                    <span className="text-muted">Total PnL</span>
                    <span
                      className={cn(
                        "text-right tabular-nums font-medium",
                        pnlColor(s.total_pnl),
                      )}
                    >
                      {formatUsd(s.total_pnl)}
                    </span>
                    <span className="text-muted">Avg PnL</span>
                    <span
                      className={cn(
                        "text-right tabular-nums",
                        pnlColor(avgPnl),
                      )}
                    >
                      {formatUsd(avgPnl)}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <tr className="border-b border-surface last:border-0">
      <td className="px-3 py-1.5 text-muted font-semibold w-32">{label}</td>
      <td className="px-3 py-1.5 tabular-nums">{value}</td>
    </tr>
  );
}
