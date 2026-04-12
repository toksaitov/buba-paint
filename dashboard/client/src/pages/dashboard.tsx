import { useOutletContext } from "react-router-dom";
import { useBotStatus } from "../hooks/use-bot-status";
import { useTrades } from "../hooks/use-trades";
import { useBalance } from "../hooks/use-balance";
import { StatCard } from "../components/dashboard/stat-card";
import { OpenTrades } from "../components/dashboard/open-trades";
import { RecentActivity } from "../components/dashboard/recent-activity";
import { MiniChart } from "../components/dashboard/mini-chart";
import { Loading } from "../components/common/loading";
import { formatUsd, formatPct, pnlColor } from "../lib/utils";

export function DashboardPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: status, isLoading: statusLoading } = useBotStatus(botId);
  const { data: tradesData } = useTrades(botId, 1, 20);
  const { data: balanceData } = useBalance(botId);

  if (statusLoading || !status) return <Loading />;

  return (
    <div className="space-y-2.5">
      {status.execution_mode === "live_readonly" && (
        <div className="border border-accent-yellow/40 bg-accent-yellow/10 px-3 py-2 text-[11px] text-accent-yellow">
          Shadow paper runtime is active. Charts, trades, signals, and stats on this page reflect
          simulated execution under <code>live_readonly</code>, while the Live page shows the real
          Polymarket venue/account state.
        </div>
      )}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-2">
        <StatCard
          label="Balance"
          value={formatUsd(status.balance)}
          sub={`HWM ${formatUsd(status.high_water_mark)}`}
        />
        <StatCard
          label="Total PnL"
          value={formatUsd(status.total_pnl)}
          color={pnlColor(status.total_pnl)}
        />
        <StatCard
          label="Win Rate"
          value={`${(status.win_rate * 100).toFixed(1)}%`}
          sub={`${status.wins}W / ${status.losses}L of ${status.total_trades}`}
        />
        <StatCard
          label="Max Drawdown"
          value={formatPct(-status.max_drawdown_pct * 100)}
          color="text-accent-red"
          sub={`${status.uptime_hours.toFixed(1)}h uptime`}
        />
      </div>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
        <MiniChart entries={balanceData?.entries ?? []} />
        <OpenTrades trades={tradesData?.trades ?? []} />
      </div>
      <RecentActivity trades={tradesData?.trades ?? []} />
    </div>
  );
}
