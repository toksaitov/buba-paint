import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import { TradeTable } from "../components/trades/trade-table";
import { PageHeader } from "../components/ui/dashboard-primitives";
import { useTrades } from "../hooks/use-trades";

export function TradesPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [page, setPage] = useState(1);
  const { data, isLoading } = useTrades(botId, page);

  if (isLoading || !data) return <Loading label="Loading trades" />;

  return (
    <div className="space-y-3">
      <PageHeader
        title="Trade history"
        description="Simulated trade history."
      />
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
