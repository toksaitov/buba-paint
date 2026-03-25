import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useTrades } from "../hooks/use-trades";
import { TradeTable } from "../components/trades/trade-table";
import { Loading } from "../components/common/loading";

export function TradesPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [page, setPage] = useState(1);
  const { data, isLoading } = useTrades(botId, page);

  if (isLoading || !data) return <Loading />;

  return (
    <div className="space-y-3">
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
