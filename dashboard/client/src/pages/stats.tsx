import { useQuery } from "@tanstack/react-query";
import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import {
  PageHeader,
  SectionCard,
} from "../components/ui/dashboard-primitives";
import { getStats } from "../lib/api";
import { cn, formatUsd, pnlColor } from "../lib/utils";

export function StatsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: stats, isLoading } = useQuery({
    queryKey: ["stats", botId],
    queryFn: () => getStats(botId),
    enabled: !!botId,
  });

  if (isLoading || !stats) return <Loading label="Loading strategies" />;

  const strategies = Object.entries(stats.by_strategy)
    .map(([name, strategy]) => ({
      name,
      ...strategy,
      avg_pnl: strategy.trades > 0 ? strategy.total_pnl / strategy.trades : 0,
    }))
    .sort((a, b) => b.total_pnl - a.total_pnl);
  const best = strategies[0] ?? null;
  const worst = strategies[strategies.length - 1] ?? null;
  const hasMultiple = strategies.length > 1;

  return (
    <div className="space-y-3">
      <PageHeader
        title="Strategies"
        description="Ranked strategy contribution and risk context for the shadow run."
      />

      <SectionCard title="Strategy contribution">
        {hasMultiple && (best || worst) && (
          <div className="mb-4 grid gap-2 md:grid-cols-2">
            {best && (
              <div className="border border-border p-3">
                <div className="text-[11px] text-muted">Best contributor</div>
                <div className="mt-1 flex items-center justify-between gap-3 text-[13px]">
                  <span className="font-semibold">{best.name}</span>
                  <span className={cn("font-semibold tabular-nums", pnlColor(best.total_pnl))}>
                    {formatUsd(best.total_pnl)}
                  </span>
                </div>
              </div>
            )}
            {worst && worst !== best && (
              <div className="border border-border p-3">
                <div className="text-[11px] text-muted">Worst contributor</div>
                <div className="mt-1 flex items-center justify-between gap-3 text-[13px]">
                  <span className="font-semibold">{worst.name}</span>
                  <span className={cn("font-semibold tabular-nums", pnlColor(worst.total_pnl))}>
                    {formatUsd(worst.total_pnl)}
                  </span>
                </div>
              </div>
            )}
          </div>
        )}
        <div className="grid gap-3 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {strategies.map((strategy) => (
            <div key={strategy.name} className="border border-border p-4">
              <div className="flex items-center justify-between gap-3">
                <div className="text-[13px] font-semibold tracking-tight">{strategy.name}</div>
                <div className={cn("text-[13px] font-semibold tabular-nums", pnlColor(strategy.total_pnl))}>
                  {formatUsd(strategy.total_pnl)}
                </div>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-y-2 text-[12px]">
                <span className="text-muted">Trades</span>
                <span className="text-right tabular-nums">{strategy.trades}</span>
                <span className="text-muted">Win rate</span>
                <span className="text-right tabular-nums">
                  {(strategy.win_rate * 100).toFixed(1)}%
                </span>
                <span className="text-muted">W/L</span>
                <span className="text-right tabular-nums">
                  {strategy.wins}/{strategy.losses}
                </span>
                <span className="text-muted">Avg PnL</span>
                <span className={cn("text-right tabular-nums", pnlColor(strategy.avg_pnl))}>
                  {formatUsd(strategy.avg_pnl)}
                </span>
              </div>
            </div>
          ))}
        </div>
        {hasMultiple && (
          <p className="mt-3 text-[11px] text-muted">
            Ranked by signed PnL, highest first.
          </p>
        )}
      </SectionCard>
    </div>
  );
}
