import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import { EquityChart } from "../components/equity/equity-chart";
import { Surface } from "../components/ui/dashboard-primitives";
import { useEquitySeries } from "../hooks/use-equity-series";

export function EquityPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: equityData, isLoading } = useEquitySeries(botId);

  if (isLoading || !equityData) return <Loading label="Loading trend" />;

  return (
    <div className="flex h-full flex-col">
      <Surface className="flex flex-1 flex-col p-3">
        <EquityChart entries={equityData.points} />
      </Surface>
    </div>
  );
}
