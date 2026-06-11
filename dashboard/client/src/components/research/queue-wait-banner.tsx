import { useEffect, useMemo, useState } from "react";
import { Banner } from "../ui/dashboard-primitives";
import {
  useResearchMachines,
  useResearchMachineTelemetry,
} from "../../hooks/use-research-machines";
import type { ResearchJob } from "../../lib/research-types";
import { formatDurationShort } from "../../lib/utils";

const UNCLAIMED_WARNING_MS = 3 * 60 * 1000;
const TICK_MS = 15_000;

interface QueueWaitBannerProps {
  job: ResearchJob;
}

function heartbeatSentence(
  workerStatus: string | undefined,
  heartbeatAgeMs: number | null,
): string {
  if (heartbeatAgeMs == null) {
    return "No research worker heartbeat has been recorded yet.";
  }
  const age = formatDurationShort(heartbeatAgeMs);
  const status = workerStatus ? workerStatus.toLowerCase() : "unknown";
  return `Research worker last heartbeat: ${age} ago (${status}).`;
}

export function QueueWaitBanner({ job }: QueueWaitBannerProps) {
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), TICK_MS);
    return () => window.clearInterval(id);
  }, []);

  const queued = job.status === "queued";
  const machinesQuery = useResearchMachines();
  const machines = machinesQuery.data?.machines;
  const workerMachineId = useMemo(() => {
    if (!machines || machines.length === 0) return "";
    const research = machines.find((machine) => machine.role === "research");
    return (research ?? machines[0]).id;
  }, [machines]);
  const telemetryQuery = useResearchMachineTelemetry(
    workerMachineId,
    queued && workerMachineId !== "",
  );

  if (!queued) return null;

  const telemetry = telemetryQuery.data?.telemetry ?? null;
  const telemetryStale = telemetryQuery.data?.stale ?? false;
  const heartbeatAgeMs =
    telemetry == null ? null : Math.max(0, nowMs - telemetry.last_heartbeat_ms);
  const queueAgeMs = Math.max(0, nowMs - job.created_at);
  const queueAge = formatDurationShort(queueAgeMs);
  const heartbeat = heartbeatSentence(telemetry?.worker_status, heartbeatAgeMs);

  if (telemetry == null || telemetryStale) {
    return (
      <Banner tone="warning" title="Waiting for a worker">
        Queued for {queueAge}. {heartbeat} Until a worker reports in, this job
        cannot start. Check the research host or cancel the job.
      </Banner>
    );
  }
  if (queueAgeMs >= UNCLAIMED_WARNING_MS) {
    return (
      <Banner tone="warning" title="No worker has claimed this job">
        Queued for {queueAge}. {heartbeat} A live worker has not picked this
        job up. Confirm the research worker can reach this dashboard queue, or
        cancel the job.
      </Banner>
    );
  }
  return (
    <Banner tone="info" title="Waiting for a worker">
      Queued for {queueAge}. {heartbeat} The next idle worker tick should claim
      this job.
    </Banner>
  );
}
