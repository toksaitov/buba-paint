import { useOutletContext } from "react-router-dom";
import { useBotStatus } from "../hooks/use-bot-status";
import { useSignals } from "../hooks/use-signals";
import { SignalTable } from "../components/signals/signal-table";
import { Loading } from "../components/common/loading";

export function SignalsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: status } = useBotStatus(botId);
  const { data, isLoading } = useSignals(botId);

  if (isLoading || !data) return <Loading />;

  return (
    <div className="space-y-3">
      {status?.execution_mode === "live_readonly" && (
        <div className="border border-accent-yellow/40 bg-accent-yellow/10 px-3 py-2 text-[11px] text-accent-yellow">
          Signal log on this page reflects the shared runtime under <code>live_readonly</code>.
          Venue/account truth still lives on the Live page.
        </div>
      )}
      <h2 className="text-[14px] font-bold">Signal Log</h2>
      <SignalTable signals={data.signals} />
    </div>
  );
}
