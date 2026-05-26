import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { cn, humanize } from "../../lib/utils";
import { Popover } from "../ui/popover";
import type { ChipTone } from "../../lib/research-permissions";

const chipBaseClasses =
  "inline-flex items-center gap-1.5 border px-2 py-0.5 text-[11px] transition-colors";

const dotToneClasses: Record<ChipTone, string> = {
  neutral: "bg-text",
  muted: "bg-muted",
  success: "bg-accent-green",
  warning: "bg-accent-blue",
  danger: "bg-accent-red",
};

const activeChipClasses = "border-text bg-bg text-text";

const inactiveChipClasses =
  "border-border bg-bg text-muted opacity-60 hover:opacity-100 hover:border-text hover:text-text";

interface StatusFilterProps {
  label: string;
  statuses: readonly string[];
  active: string[];
  onChange: (active: string[]) => void;
  toneFor?: (status: string) => ChipTone;
  labelFor?: (status: string) => string;
  ariaLabel?: string;
}

function describeActive(
  statuses: readonly string[],
  active: string[],
  labelFor: (status: string) => string,
): string {
  if (active.length === 0) return "none";
  if (active.length === statuses.length) return "all";
  if (active.length <= 3) {
    const ordered = statuses.filter((s) => active.includes(s));
    return ordered.map(labelFor).join(", ");
  }
  return `${active.length} of ${statuses.length}`;
}

export function StatusFilter({
  label,
  statuses,
  active,
  onChange,
  toneFor,
  labelFor,
  ariaLabel,
}: StatusFilterProps) {
  const [open, setOpen] = useState(false);
  const [anchorEl, setAnchorEl] = useState<HTMLButtonElement | null>(null);
  const resolveLabel = labelFor ?? humanize;
  const summary = describeActive(statuses, active, resolveLabel);
  const triggerLabel = ariaLabel ?? `${label} filter`;

  const toggle = (status: string) => {
    if (active.includes(status)) {
      onChange(active.filter((s) => s !== status));
    } else {
      onChange([...active, status]);
    }
  };

  return (
    <>
      <button
        ref={setAnchorEl}
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-label={triggerLabel}
        aria-expanded={open}
        className="mb-2 inline-flex items-center gap-1 self-start text-[11px] text-muted hover:text-text"
      >
        <span>{label}:</span>
        <span className="text-text">{summary}</span>
        <ChevronDown
          size={11}
          aria-hidden
          className={cn(
            "text-muted transition-transform",
            open && "rotate-180",
          )}
        />
      </button>
      <Popover
        anchor={anchorEl}
        open={open}
        onClose={() => setOpen(false)}
        width={320}
        ariaLabel={triggerLabel}
      >
        <div className="flex flex-wrap items-center gap-1.5 p-2">
          {statuses.map((status) => {
            const isActive = active.includes(status);
            const tone: ChipTone = toneFor?.(status) ?? "neutral";
            return (
              <button
                key={status}
                type="button"
                onClick={() => toggle(status)}
                aria-pressed={isActive}
                className={cn(
                  chipBaseClasses,
                  isActive ? activeChipClasses : inactiveChipClasses,
                )}
              >
                {isActive && (
                  <span
                    aria-hidden
                    className={cn(
                      "h-1.5 w-1.5 rounded-full",
                      dotToneClasses[tone],
                    )}
                  />
                )}
                {resolveLabel(status)}
              </button>
            );
          })}
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-border px-2 py-1 text-[11px] text-muted">
          <button
            type="button"
            onClick={() => onChange([...statuses])}
            className="hover:text-text hover:underline"
          >
            all
          </button>
          <span aria-hidden>·</span>
          <button
            type="button"
            onClick={() => onChange([])}
            className="hover:text-text hover:underline"
          >
            clear
          </button>
        </div>
      </Popover>
    </>
  );
}
