import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import { SignalGroupTable, SignalTable } from "../components/signals/signal-table";
import { PageHeader } from "../components/ui/dashboard-primitives";
import { useSignalGroups } from "../hooks/use-signal-groups";
import { useSignals } from "../hooks/use-signals";

export function SignalsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [view, setView] = useState<"groups" | "raw">("groups");
  const { data: groupedData, isLoading: groupsLoading } = useSignalGroups(botId);
  const { data: rawData, isLoading: rawLoading } = useSignals(botId, 100);

  if (groupsLoading || !groupedData || (view === "raw" && (rawLoading || !rawData))) {
    return <Loading label="Loading signals" />;
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-col gap-2 md:flex-row md:items-end md:justify-between">
        <PageHeader
          title="Signals"
          description={`Signals grouped into bursts (quiet gap ${groupedData.quiet_gap_ms}ms, scanned ${groupedData.raw_rows_scanned.toLocaleString()} rows). Switch to Raw to see the underlying rows.`}
        />
        <div className="flex border border-border text-[11px]">
          <button
            type="button"
            onClick={() => setView("groups")}
            className={view === "groups" ? "bg-text px-2 py-1 text-bg" : "px-2 py-1 text-muted"}
          >
            Grouped
          </button>
          <button
            type="button"
            onClick={() => setView("raw")}
            className={view === "raw" ? "bg-text px-2 py-1 text-bg" : "px-2 py-1 text-muted"}
          >
            Raw
          </button>
        </div>
      </div>
      {view === "groups" ? (
        <SignalGroupTable groups={groupedData.groups} />
      ) : (
        <SignalTable signals={rawData?.signals ?? []} />
      )}
    </div>
  );
}
