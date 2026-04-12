import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useBotStatus } from "../hooks/use-bot-status";
import { useTrades } from "../hooks/use-trades";
import { TradeTable } from "../components/trades/trade-table";
import { Loading } from "../components/common/loading";

export function TradesPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [page, setPage] = useState(1);
  const { data: status } = useBotStatus(botId);
  const { data, isLoading } = useTrades(botId, page);

  if (isLoading || !data) return <Loading />;

  return (
    <div className="space-y-3">
      {status?.execution_mode === "live_readonly" && (
        <div className="border border-accent-yellow/40 bg-accent-yellow/10 px-3 py-2 text-[11px] text-accent-yellow">
          Trade history on this page is the <code>live_readonly</code> shadow paper track, not
          real venue fills.
        </div>
      )}
      <h2 className="text-[14px] font-bold">Trade History</h2>
      <TradeTable
        trades={data.trades}
        page={data.page}
        total={data.total}
        perPage={data.per_page}
        onPageChange={setPage}
      />
    </div>
  );
}
