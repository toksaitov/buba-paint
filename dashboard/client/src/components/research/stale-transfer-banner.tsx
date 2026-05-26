import { useEffect, useState } from "react";
import { Banner } from "../ui/dashboard-primitives";
import { TRANSFER_STALE_MS } from "../../lib/research-types";
import type { ArtifactTransfer } from "../../lib/research-types";
import { formatDurationShort } from "../../lib/utils";

interface StaleTransferBannerProps {
  transfer: ArtifactTransfer;
  nowMs?: number;
}

export function StaleTransferBanner({
  transfer,
  nowMs,
}: StaleTransferBannerProps) {
  const [tick, setTick] = useState<number>(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setTick(Date.now()), 30_000);
    return () => window.clearInterval(id);
  }, []);
  const effectiveNow = nowMs ?? tick;
  if (transfer.status !== "running") return null;
  const age = effectiveNow - transfer.updated_at;
  if (age < TRANSFER_STALE_MS) return null;
  return (
    <Banner tone="warning" title="Transfer may have stalled">
      No progress for {formatDurationShort(age)}. The worker recovers
      transfers older than {formatDurationShort(TRANSFER_STALE_MS)} to
      retryable on its next tick. Retry or cancel to take explicit action.
    </Banner>
  );
}
