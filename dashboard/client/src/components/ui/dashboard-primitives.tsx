import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  LabelHTMLAttributes,
  ReactNode,
  TextareaHTMLAttributes,
} from "react";
import { useEffect, useId, useRef, useState } from "react";
import { Check, Copy, Loader2, X, Info, AlertTriangle, AlertOctagon, CheckCircle } from "lucide-react";
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
    <section className="border-b border-border bg-bg px-3 py-3">
      <div className="min-w-0 space-y-1">
        <h1 className="text-[18px] font-semibold tracking-tight">{title}</h1>
        <p className="text-[12px] text-muted">{description}</p>
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

const bannerToneClasses = {
  info: "border-border bg-surface text-text",
  warning: "border-accent-blue bg-[var(--color-warning-fill)] text-text",
  danger: "border-accent-red bg-[var(--color-danger-fill)] text-text",
  success: "border-accent-green bg-[var(--color-success-fill)] text-text",
} as const;

type BannerTone = keyof typeof bannerToneClasses;

const bannerToneIcon: Record<BannerTone, ReactNode> = {
  info: <Info size={14} aria-hidden />,
  warning: <AlertTriangle size={14} aria-hidden />,
  danger: <AlertOctagon size={14} aria-hidden />,
  success: <CheckCircle size={14} aria-hidden />,
};

interface BannerProps {
  tone: BannerTone;
  title: string;
  children?: ReactNode;
  onDismiss?: () => void;
  action?: ReactNode;
  icon?: ReactNode;
  role?: "alert" | "status";
  ariaLive?: "polite" | "assertive";
  className?: string;
}

export function Banner({
  tone,
  title,
  children,
  onDismiss,
  action,
  icon,
  role,
  ariaLive,
  className,
}: BannerProps) {
  const computedRole = role ?? (tone === "danger" ? "alert" : "status");
  const computedAriaLive = ariaLive ?? (tone === "danger" ? "assertive" : "polite");
  return (
    <section
      role={computedRole}
      aria-live={computedAriaLive}
      className={cn(
        "flex flex-col gap-2 border px-3 py-2.5 md:flex-row md:items-start md:gap-3",
        bannerToneClasses[tone],
        className,
      )}
    >
      <div className="flex flex-1 items-start gap-2">
        <span className="mt-0.5 inline-flex shrink-0">{icon ?? bannerToneIcon[tone]}</span>
        <div className="min-w-0 flex-1 space-y-1">
          <div className="text-md font-semibold tracking-tight">{title}</div>
          {children && <div className="text-sm text-muted">{children}</div>}
        </div>
      </div>
      {(action || onDismiss) && (
        <div className="flex items-center gap-2 self-end md:self-start">
          {action}
          {onDismiss && (
            <button
              type="button"
              onClick={onDismiss}
              aria-label="Dismiss"
              className="inline-flex h-7 w-7 items-center justify-center border border-border text-muted hover:border-text hover:text-text"
            >
              <X size={12} aria-hidden />
            </button>
          )}
        </div>
      )}
    </section>
  );
}

const buttonToneClasses = {
  neutral: "border-border text-text hover:bg-surface",
  accent: "border-accent-blue text-accent-blue hover:bg-[var(--color-warning-fill)]",
  danger: "border-accent-red text-accent-red hover:bg-[var(--color-danger-fill)]",
} as const;

type ButtonTone = keyof typeof buttonToneClasses;
type ButtonSize = "sm" | "md";
type ButtonState = "idle" | "pending" | "success" | "error";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type"> {
  tone?: ButtonTone;
  size?: ButtonSize;
  state?: ButtonState;
  iconLeft?: ReactNode;
  iconRight?: ReactNode;
  fullWidth?: boolean;
  type?: "button" | "submit";
}

export function Button({
  tone = "neutral",
  size = "md",
  state = "idle",
  iconLeft,
  iconRight,
  fullWidth,
  type = "button",
  className,
  children,
  disabled,
  ...rest
}: ButtonProps) {
  const isPending = state === "pending";
  const isSuccess = state === "success";
  const isError = state === "error";
  const renderedLeft = isPending
    ? <Loader2 size={14} className="animate-spin" aria-hidden />
    : isSuccess
      ? <Check size={14} aria-hidden />
      : iconLeft;
  return (
    <button
      type={type}
      data-state={state}
      disabled={disabled || isPending}
      className={cn(
        "inline-flex items-center justify-center gap-2 border px-3 text-sm font-semibold tracking-tight transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        size === "md"
          ? "min-h-[44px] py-2 lg:min-h-0 lg:py-2"
          : "min-h-[28px] py-1",
        buttonToneClasses[tone],
        isSuccess && "border-accent-green text-accent-green",
        isError && "border-accent-red text-accent-red",
        fullWidth && "w-full",
        className,
      )}
      {...rest}
    >
      {renderedLeft && <span className="inline-flex shrink-0">{renderedLeft}</span>}
      <span className="min-w-0">{children}</span>
      {iconRight && <span className="inline-flex shrink-0">{iconRight}</span>}
    </button>
  );
}

interface SpinnerProps {
  size?: number;
  className?: string;
  ariaLabel?: string;
}

export function Spinner({ size = 14, className, ariaLabel = "Loading" }: SpinnerProps) {
  return (
    <Loader2
      size={size}
      aria-label={ariaLabel}
      className={cn("animate-spin", className)}
    />
  );
}

const inputStateClasses: Record<NonNullable<InputProps["state"]>, string> = {
  idle: "border-border focus:border-text",
  valid: "border-accent-green focus:border-accent-green",
  invalid: "border-accent-red focus:border-accent-red",
};

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  state?: "idle" | "valid" | "invalid";
}

export function Input({ state = "idle", className, ...rest }: InputProps) {
  return (
    <input
      data-state={state}
      className={cn(
        "w-full border bg-bg px-2 py-1.5 text-sm outline-none transition-colors",
        inputStateClasses[state],
        className,
      )}
      {...rest}
    />
  );
}

interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  state?: "idle" | "valid" | "invalid";
  minRows?: number;
}

export function Textarea({
  state = "idle",
  minRows = 2,
  className,
  rows,
  ...rest
}: TextareaProps) {
  return (
    <textarea
      data-state={state}
      rows={rows ?? minRows}
      className={cn(
        "w-full border bg-bg px-2 py-1.5 text-sm outline-none transition-colors",
        inputStateClasses[state],
        className,
      )}
      {...rest}
    />
  );
}

interface LabelProps extends LabelHTMLAttributes<HTMLLabelElement> {
  required?: boolean;
}

export function Label({
  required,
  className,
  children,
  ...rest
}: LabelProps) {
  return (
    <label
      className={cn("relative inline-flex items-center gap-2 text-xs font-semibold tracking-tight", className)}
      {...rest}
    >
      <span>{children}</span>
      {required && <span className="sr-only">required</span>}
    </label>
  );
}

interface FormFieldProps {
  label: string;
  hint?: string;
  error?: string;
  required?: boolean;
  htmlFor?: string;
  children: (props: { id: string; describedBy?: string }) => ReactNode;
  className?: string;
}

export function FormField({
  label,
  hint,
  error,
  required,
  htmlFor,
  children,
  className,
}: FormFieldProps) {
  const generatedId = useId();
  const id = htmlFor ?? generatedId;
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = errorId ?? hintId;
  return (
    <div className={cn("space-y-1", className)}>
      <Label htmlFor={id} required={required}>
        {label}
      </Label>
      {children({ id, describedBy })}
      {error ? (
        <p id={errorId} className="text-xs text-accent-red">
          {error}
        </p>
      ) : hint ? (
        <p id={hintId} className="text-xs text-muted">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

interface CopyButtonProps {
  value: string;
  ariaLabel?: string;
  size?: "sm" | "md";
  className?: string;
}

export function CopyButton({
  value,
  ariaLabel = "Copy",
  size = "md",
  className,
}: CopyButtonProps) {
  const [state, setState] = useState<"idle" | "success" | "error">("idle");
  const timeoutRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current != null) {
        window.clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  async function handleCopy() {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
      } else {
        const textarea = document.createElement("textarea");
        textarea.value = value;
        textarea.setAttribute("readonly", "");
        textarea.style.position = "absolute";
        textarea.style.left = "-9999px";
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        document.body.removeChild(textarea);
      }
      setState("success");
    } catch {
      setState("error");
    }
    if (timeoutRef.current != null) {
      window.clearTimeout(timeoutRef.current);
    }
    timeoutRef.current = window.setTimeout(() => setState("idle"), 1500);
  }

  const iconSize = size === "sm" ? 12 : 14;
  return (
    <button
      type="button"
      onClick={handleCopy}
      aria-label={state === "success" ? "Copied" : ariaLabel}
      data-state={state}
      className={cn(
        "relative inline-flex items-center justify-center border border-border transition-colors hover:border-text",
        size === "sm" ? "h-6 w-6" : "h-7 w-7",
        state === "success" && "border-accent-green text-accent-green",
        state === "error" && "border-accent-red text-accent-red",
        className,
      )}
    >
      {state === "success" ? (
        <Check size={iconSize} aria-hidden />
      ) : (
        <Copy size={iconSize} aria-hidden />
      )}
      <span className="sr-only" aria-live="polite">
        {state === "success" ? "Copied" : ""}
      </span>
    </button>
  );
}

interface ProgressBarProps {
  value: number;
  ariaLabel: string;
  tone?: "neutral" | "warning" | "danger";
  thresholds?: { warning: number; danger: number };
  showTick?: boolean;
  className?: string;
}

export function ProgressBar({
  value,
  ariaLabel,
  tone,
  thresholds = { warning: 0.7, danger: 0.85 },
  showTick = true,
  className,
}: ProgressBarProps) {
  const clamped = Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0;
  const computedTone =
    tone ?? (clamped >= thresholds.danger ? "danger" : clamped >= thresholds.warning ? "warning" : "neutral");
  const fillClass =
    computedTone === "danger"
      ? "bg-accent-red"
      : computedTone === "warning"
        ? "bg-accent-blue"
        : "bg-text";
  return (
    <div
      role="progressbar"
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={clamped}
      className={cn("relative h-1 w-full bg-surface", className)}
    >
      <div
        className={cn("absolute inset-y-0 left-0 transition-[width]", fillClass)}
        style={{ width: `${clamped * 100}%` }}
      />
      {showTick && (
        <span
          aria-hidden
          className="absolute inset-y-0 w-px bg-muted"
          style={{ left: "80%" }}
        />
      )}
    </div>
  );
}

interface SegmentItem<T extends string> {
  value: T;
  label: string;
}

interface SegmentProps<T extends string> {
  value: T;
  onChange: (next: T) => void;
  items: SegmentItem<T>[];
  ariaLabel: string;
  size?: "sm" | "md";
  className?: string;
}

export function Segment<T extends string>({
  value,
  onChange,
  items,
  ariaLabel,
  size = "sm",
  className,
}: SegmentProps<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn("inline-flex border border-border", className)}
    >
      {items.map((item) => {
        const active = item.value === value;
        return (
          <button
            key={item.value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(item.value)}
            className={cn(
              "border-r border-border last:border-r-0 transition-colors",
              size === "sm" ? "px-2 py-1 text-xs" : "px-3 py-1.5 text-sm",
              active ? "bg-text text-bg font-semibold" : "text-muted hover:text-text",
            )}
          >
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

interface RelativeTimeProps {
  epochMs: number | null | undefined;
  staleAfterMs?: number;
  className?: string;
  fallback?: string;
}

function formatRelative(ageMs: number): string {
  if (ageMs < 0) ageMs = 0;
  const seconds = Math.floor(ageMs / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function pickInterval(ageMs: number): number {
  if (ageMs < 60_000) return 1000;
  if (ageMs < 3_600_000) return 30_000;
  return 300_000;
}

export function RelativeTime({
  epochMs,
  staleAfterMs,
  className,
  fallback = "n/a",
}: RelativeTimeProps) {
  const [now, setNow] = useState<number>(() => Date.now());

  useEffect(() => {
    if (epochMs == null) return;
    const update = () => setNow(Date.now());
    const age = Math.max(0, Date.now() - epochMs);
    const intervalMs = pickInterval(age);
    const id = window.setInterval(update, intervalMs);
    return () => window.clearInterval(id);
  }, [epochMs, now]);

  if (epochMs == null) {
    return <span className={cn("text-muted", className)}>{fallback}</span>;
  }
  const age = Math.max(0, now - epochMs);
  const stale = staleAfterMs != null && age > staleAfterMs;
  return (
    <time
      dateTime={new Date(epochMs).toISOString()}
      className={cn("tabular-nums", stale ? "text-accent-red" : "text-muted", className)}
    >
      {formatRelative(age)}
    </time>
  );
}
