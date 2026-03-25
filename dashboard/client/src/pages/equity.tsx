import { useOutletContext } from "react-router-dom";
import { useBalance } from "../hooks/use-balance";
import { useBotStatus } from "../hooks/use-bot-status";
import { EquityChart } from "../components/equity/equity-chart";
import { Loading } from "../components/common/loading";
import { formatUsd, pnlColor } from "../lib/utils";

export function EquityPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: balanceData, isLoading } = useBalance(botId);
  const { data: status } = useBotStatus(botId);

  if (isLoading || !balanceData) return <Loading />;

  return (
    <div className="space-y-3">
      <div className="flex items-baseline gap-4">
        <h2 className="text-[14px] font-bold">Equity Curve</h2>
        {status && (
          <span className="text-[12px] text-muted">
            Balance:{" "}
            <span className={`font-semibold ${pnlColor(status.total_pnl)}`}>
              {formatUsd(status.balance)}
            </span>
          </span>
        )}
      </div>
      <div className="border border-border p-4 bg-bg">
        <EquityChart entries={balanceData.entries} />
      </div>
    </div>
  );
}
