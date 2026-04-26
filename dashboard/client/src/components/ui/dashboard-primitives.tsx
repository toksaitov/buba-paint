import type { ReactNode } from "react";
import { cn } from "../../lib/utils";
import type { TradingAlert } from "../../lib/types";

const chipToneClasses = {
  neutral: "border-border text-text",
  muted: "border-border text-muted",
  success: "border-accent-green text-accent-green",
  warning: "border-accent-blue text-accent-blue",
  danger: "border-accent-red text-accent-red",
} as const;

type ChipTone = keyof typeof chipToneClasses;

interface InfoHintProps {
  label: string;
  text: string;
  className?: string;
}

export function InfoHint({ label, text, className }: InfoHintProps) {
  return (
    <button
      type="button"
      title={text}
      aria-label={`${label}: ${text}`}
      className={cn(
        "inline-flex h-3.5 w-3.5 items-center justify-center rounded-full border border-border text-[9px] text-muted hover:border-text hover:text-text",
        className,
      )}
    >
      ?
    </button>
  );
}

interface StatusChipProps {
  label: string;
  tone?: ChipTone;
  dot?: boolean;
  title?: string;
  compact?: boolean;
  help?: string;
}

export function StatusChip({
  label,
  tone = "neutral",
  dot = false,
  title,
  compact = false,
  help,
}: StatusChipProps) {
  const chip = (
    <span
      title={title}
      className={cn(
        "inline-flex items-center gap-1.5 border text-[11px] font-medium",
        compact ? "px-1.5 py-0.5" : "px-2 py-1",
        chipToneClasses[tone],
      )}
    >
      {dot && (
        <span
          className={cn(
            "h-1.5 w-1.5 rounded-full",
            tone === "success"
              ? "bg-accent-green"
                : tone === "danger"
                  ? "bg-accent-red"
                  : tone === "warning"
                    ? "bg-accent-blue"
                    : "bg-text",
          )}
        />
      )}
      {label}
    </span>
  );
  if (!help) return chip;
  return (
    <span className="inline-flex items-center gap-1">
      {chip}
      <InfoHint label={label} text={help} />
    </span>
  );
}

interface ContextStripProps {
  title: string;
  description: string;
}

export function ContextStrip({
  title,
  description,
}: ContextStripProps) {
  return (
    <section className="border-b border-border bg-bg px-3 py-2 md:px-4">
      <div className="min-w-0">
        <h1 className="text-[15px] font-semibold tracking-tight">{title}</h1>
        <div className="mt-1 text-[12px] text-muted">{description}</div>
      </div>
    </section>
  );
}

interface PageHeaderProps {
  title: string;
  description?: string;
  className?: string;
}

export function PageHeader({
  title,
  description,
  className,
}: PageHeaderProps) {
  return (
    <div className={cn("space-y-1", className)}>
      <h1 className="text-[18px] font-semibold tracking-tight">{title}</h1>
      {description && <p className="text-[12px] text-muted">{description}</p>}
    </div>
  );
}

interface SurfaceProps {
  children: ReactNode;
  className?: string;
}

export function Surface({ children, className }: SurfaceProps) {
  return <section className={cn("border border-border bg-bg", className)}>{children}</section>;
}

interface SectionCardProps {
  title: string;
  subtitle?: string;
  toolbar?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function SectionCard({
  title,
  subtitle,
  toolbar,
  children,
  className,
}: SectionCardProps) {
  return (
    <section className={cn("border border-border bg-bg", className)}>
      <div className="flex items-start justify-between gap-3 border-b border-border px-3 py-2.5">
        <div className="min-w-0">
          <h2 className="text-[14px] font-semibold tracking-tight">{title}</h2>
          {subtitle && <p className="mt-1 text-[11px] text-muted">{subtitle}</p>}
        </div>
        {toolbar && <div className="shrink-0">{toolbar}</div>}
      </div>
      <div className="p-3 flex-1 min-h-0 flex flex-col">{children}</div>
    </section>
  );
}

interface MetricCardProps {
  label: string;
  value: string;
  sub?: string;
  tone?: ChipTone;
  help?: string;
}

export function MetricCard({
  label,
  value,
  sub,
  tone = "neutral",
  help,
}: MetricCardProps) {
  const valueColor =
    tone === "success"
      ? "text-accent-green"
      : tone === "danger"
        ? "text-accent-red"
        : tone === "warning"
          ? "text-accent-blue"
          : "text-text";

  return (
    <div className="border border-border bg-bg px-3 py-3">
      <div className="flex items-center gap-1.5 text-[11px] text-muted">
        <span>{label}</span>
        {help && <InfoHint label={label} text={help} />}
      </div>
      <div className={cn("mt-1.5 text-[22px] font-bold tracking-tight tabular-nums", valueColor)}>
        {value}
      </div>
      {sub && <div className="mt-1 text-[11px] text-muted tabular-nums">{sub}</div>}
    </div>
  );
}

interface AlertListProps {
  alerts: TradingAlert[];
  emptyMessage?: string;
}

export function AlertList({
  alerts,
  emptyMessage = "No active alerts.",
}: AlertListProps) {
  if (alerts.length === 0) {
    return <StateEmpty message={emptyMessage} />;
  }

  return (
    <div className="space-y-2">
      {alerts.map((alert, index) => (
        <div key={`${alert.severity}-${alert.title}-${index}`} className="border border-border p-3">
          <div className="flex items-center gap-2">
            <StatusChip
              label={alert.severity}
              tone={
                alert.severity === "critical"
                  ? "danger"
                : alert.severity === "warning"
                    ? "warning"
                    : "muted"
              }
              compact
            />
            <div className="text-[12px] font-semibold tracking-tight">{alert.title}</div>
          </div>
          <div className="mt-2 text-[12px] text-muted">{alert.detail}</div>
        </div>
      ))}
    </div>
  );
}

interface KeyValueItem {
  label: string;
  value: ReactNode;
  tone?: ChipTone;
  help?: string;
}

interface KeyValueListProps {
  items: KeyValueItem[];
  columns?: 1 | 2;
}

export function KeyValueList({ items, columns = 1 }: KeyValueListProps) {
  return (
    <div className={cn("grid gap-x-6 gap-y-2", columns === 2 && "md:grid-cols-2")}>
      {items.map((item) => (
        <div key={item.label} className="flex items-start justify-between gap-4 text-[12px]">
          <span className="flex items-center gap-1 text-muted">
            <span>{item.label}</span>
            {item.help && <InfoHint label={item.label} text={item.help} />}
          </span>
          <span
            className={cn(
              "max-w-[65%] text-right tabular-nums",
              item.tone === "success" && "text-accent-green",
              item.tone === "danger" && "text-accent-red",
              item.tone === "warning" && "text-accent-blue",
            )}
          >
            {item.value}
          </span>
        </div>
      ))}
    </div>
  );
}

interface StateEmptyProps {
  message: string;
}

export function StateEmpty({ message }: StateEmptyProps) {
  return <div className="text-[12px] text-muted">{message}</div>;
}

interface TableToolbarProps {
  left?: ReactNode;
  right?: ReactNode;
}

export function TableToolbar({ left, right }: TableToolbarProps) {
  return (
    <div className="flex flex-col gap-2 border-b border-border px-3 py-2 md:flex-row md:items-center md:justify-between">
      <div className="min-w-0">{left}</div>
      <div className="flex flex-wrap items-center gap-2">{right}</div>
    </div>
  );
}
