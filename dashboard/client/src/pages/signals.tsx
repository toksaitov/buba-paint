import { useOutletContext } from "react-router-dom";
import { useSignals } from "../hooks/use-signals";
import { SignalTable } from "../components/signals/signal-table";
import { Loading } from "../components/common/loading";

export function SignalsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data, isLoading } = useSignals(botId);

  if (isLoading || !data) return <Loading />;

  return (
    <div className="space-y-3">
      <h2 className="text-[14px] font-bold">Signal Log</h2>
      <SignalTable signals={data.signals} />
    </div>
  );
}
