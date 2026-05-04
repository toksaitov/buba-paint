import { Link, useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import { MiniChart } from "../components/dashboard/mini-chart";
import { OpenTrades } from "../components/dashboard/open-trades";
import { RecentActivity } from "../components/dashboard/recent-activity";
import {
  AlertList,
  KeyValueList,
  MetricCard,
  SectionCard,
  StateEmpty,
} from "../components/ui/dashboard-primitives";
import { useEquitySeries } from "../hooks/use-equity-series";
import { useTrades } from "../hooks/use-trades";
import { useTradingSummary } from "../hooks/use-trading-summary";
import {
  processStateLabel,
  runtimeModeLabel,
} from "../lib/trading-summary";
import { empty, help } from "../lib/copy";
import { formatDateTime, formatPct, formatUsd } from "../lib/utils";

function SectionLink({ to, children }: { to: string; children: string }) {
  return (
    <Link to={to} className="text-[11px] text-muted hover:text-text">
      {children}
    </Link>
  );
}

export function DashboardPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: summary, isLoading } = useTradingSummary(botId);
  const { data: tradesData } = useTrades(botId, 1, 20);
  const { data: equityData } = useEquitySeries(botId);

  if (isLoading || !summary) return <Loading label="Loading summary" />;

  const shadow = summary.shadow_summary;
  const account = summary.real_account_summary;
  const hasAlerts = summary.alerts.length > 0;
  const isPaper = summary.runtime_mode === "paper";
  const hasAccountSnapshot = account.latest_snapshot_at_ms != null;
  const lastRefreshAtMs =
    account.last_account_refresh_at_ms ?? account.latest_snapshot_at_ms;
  const alertCount = summary.alerts.length;
  const remainingAlerts = Math.max(0, alertCount - 3);

  return (
    <div className="space-y-3">
      {hasAlerts && (
        <SectionCard
          title="Alerts"
          toolbar={
            remainingAlerts > 0 ? (
              <SectionLink to="/execution">{`View all ${alertCount}`}</SectionLink>
            ) : undefined
          }
        >
          <AlertList alerts={summary.alerts.slice(0, 3)} emptyMessage={empty.noActiveAlerts} />
        </SectionCard>
      )}

      <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
        <MetricCard
          label="Balance"
          value={formatUsd(shadow.balance)}
          sub={`HWM ${formatUsd(shadow.high_water_mark)}`}
        />
        <MetricCard
          label="PnL"
          value={formatUsd(shadow.total_pnl)}
          tone={shadow.total_pnl > 0 ? "success" : shadow.total_pnl < 0 ? "danger" : "neutral"}
          sub={`${shadow.total_trades} settled trades`}
        />
        <MetricCard
          label="Win rate"
          value={`${(shadow.win_rate * 100).toFixed(1)}%`}
          sub={`${shadow.wins}W / ${shadow.losses}L`}
          help={help.winRate}
        />
        <MetricCard
          label="Max drawdown"
          value={formatPct(-shadow.max_drawdown_pct * 100)}
          tone="danger"
          sub={`${shadow.uptime_hours.toFixed(1)}h uptime`}
          help={help.maxDrawdown}
        />
      </div>

      <div className="grid gap-3 xl:grid-cols-[1.6fr_1fr]">
        <SectionCard
          title="Equity curve"
          className="xl:flex xl:flex-col"
          toolbar={<SectionLink to="/equity">Open Trend</SectionLink>}
        >
          <div className="xl:flex-1 xl:min-h-[260px]">
            <MiniChart entries={equityData?.points ?? []} />
          </div>
        </SectionCard>

        <div className="space-y-3">
          {shadow.current_window && (
            <SectionCard title="Current market">
              <KeyValueList
                items={[
                  {
                    label: "Question",
                    value: shadow.current_window.question ?? "n/a",
                  },
                  {
                    label: "Ends",
                    value: formatDateTime(shadow.current_window.end_time),
                  },
                  {
                    label: "Market ID",
                    value: shadow.current_window.market_id ?? "n/a",
                  },
                ]}
              />
            </SectionCard>
          )}

          <SectionCard title="Open shadow trades">
            <OpenTrades trades={tradesData?.trades ?? []} />
          </SectionCard>

          {isPaper ? (
            <SectionCard title="Execution">
              <KeyValueList
                items={[
                  {
                    label: "Process",
                    value: processStateLabel(summary.process_state),
                  },
                  { label: "Mode", value: runtimeModeLabel(summary.runtime_mode) },
                  { label: "Open shadow trades", value: shadow.open_trades.toString() },
                ]}
              />
            </SectionCard>
          ) : (
            <SectionCard
              title="Polymarket account"
              toolbar={<SectionLink to="/execution">Open Execution</SectionLink>}
            >
              {hasAccountSnapshot ? (
                <KeyValueList
                  items={[
                    {
                      label: "Cash available",
                      value:
                        account.available_cash != null
                          ? formatUsd(account.available_cash)
                          : "n/a",
                      help: help.cashAvailable,
                    },
                    {
                      label: "Total equity",
                      value:
                        account.total_equity != null ? formatUsd(account.total_equity) : "n/a",
                    },
                    {
                      label: "Open venue orders",
                      value: account.open_orders.toString(),
                    },
                    {
                      label: "Last refresh",
                      value:
                        lastRefreshAtMs != null ? formatDateTime(lastRefreshAtMs) : "n/a",
                    },
                  ]}
                />
              ) : (
                <StateEmpty message={empty.noPolymarketSnapshot} />
              )}
            </SectionCard>
          )}
        </div>
      </div>

      <SectionCard
        title="Recent settled trades"
        toolbar={<SectionLink to="/trades">Open Trades</SectionLink>}
      >
        <RecentActivity trades={tradesData?.trades ?? []} />
      </SectionCard>
    </div>
  );
}
