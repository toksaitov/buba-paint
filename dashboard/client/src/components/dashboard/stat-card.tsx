import { cn } from "../../lib/utils";

interface StatCardProps {
  label: string;
  value: string;
  sub?: string;
  color?: string;
}

export function StatCard({ label, value, sub, color }: StatCardProps) {
  return (
    <div className="border border-border px-3 py-2.5 bg-bg">
      <div className="text-[10px] uppercase tracking-wide text-muted mb-1">
        {label}
      </div>
      <div className={cn("text-xl font-bold tabular-nums", color)}>
        {value}
      </div>
      {sub && (
        <div className="text-[11px] text-muted mt-0.5 tabular-nums">{sub}</div>
      )}
    </div>
  );
}
