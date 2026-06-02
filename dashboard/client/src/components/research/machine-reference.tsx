import { Link } from "react-router-dom";
import type { ResearchMachine } from "../../lib/research-types";

interface MachineReferenceProps {
  machineId: string | null | undefined;
  machines: ResearchMachine[];
}

export function MachineReference({ machineId, machines }: MachineReferenceProps) {
  if (!machineId) return <span className="text-muted">-</span>;
  const machine = machines.find((m) => m.id === machineId);
  if (machine?.role === "research") {
    return (
      <Link
        to={`/research/machines/${encodeURIComponent(machineId)}`}
        className="font-mono text-[11px] hover:underline"
      >
        {machineId}
      </Link>
    );
  }
  return <span className="font-mono text-[11px] text-muted">{machineId}</span>;
}
