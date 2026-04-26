import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import { EquityChart } from "../components/equity/equity-chart";
import {
  MetricCard,
  PageHeader,
  Surface,
} from "../components/ui/dashboard-primitives";
import { useEquitySeries } from "../hooks/use-equity-series";
import { useTradingSummary } from "../hooks/use-trading-summary";
import { formatPct, formatUsd } from "../lib/utils";
import { help } from "../lib/copy";

export function EquityPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: equityData, isLoading } = useEquitySeries(botId);
  const { data: summary } = useTradingSummary(botId);

  if (isLoading || !equityData || !summary) return <Loading label="Loading equity curve" />;

  const shadow = summary.shadow_summary;

  return (
    <div className="space-y-3">
      <PageHeader
        title="Equity curve"
        description="Simulated balance over time. Starting balance is the baseline."
      />
      <div className="grid gap-2 md:grid-cols-3">
        <MetricCard
          label="Starting balance"
          value={formatUsd(shadow.starting_balance)}
          help={help.baseline}
        />
        <MetricCard label="Current balance" value={formatUsd(shadow.balance)} />
        <MetricCard
          label="Max drawdown"
          value={formatPct(-shadow.max_drawdown_pct * 100)}
          tone="danger"
          help={help.maxDrawdown}
        />
      </div>
      <Surface className="p-3">
        <EquityChart entries={equityData.points} />
      </Surface>
    </div>
  );
}
