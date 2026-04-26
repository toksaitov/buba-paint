import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { connectWs } from "../lib/ws";
import { showTradeNotification } from "../lib/notifications";

export function useLiveUpdates(botId: string) {
  const qc = useQueryClient();
  const disabled = useRef(false);

  useEffect(() => {
    if (!botId || disabled.current) return;

    const cleanup = connectWs(
      botId,
      (msg: unknown) => {
        const m = msg as { type?: string; side?: string; strategy?: string; pnl?: number | null };
        if (m.type === "trade") {
          void qc.invalidateQueries({ queryKey: ["trades", botId] });
          void qc.invalidateQueries({ queryKey: ["bot-status", botId] });
          showTradeNotification(m);
        } else if (m.type === "balance") {
          void qc.invalidateQueries({ queryKey: ["balance", botId] });
          void qc.invalidateQueries({ queryKey: ["equity-series", botId] });
          void qc.invalidateQueries({ queryKey: ["bot-status", botId] });
        } else if (m.type === "signal") {
          void qc.invalidateQueries({ queryKey: ["signals", botId] });
          void qc.invalidateQueries({ queryKey: ["signal-groups", botId] });
        }
      },
      () => {
        disabled.current = true;
      },
    );

    return cleanup;
  }, [botId, qc]);
}
