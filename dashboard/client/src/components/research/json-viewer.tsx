import { useMemo } from "react";
import { CopyButton } from "../ui/dashboard-primitives";

interface JsonViewerProps {
  value: unknown;
  label?: string;
  emptyLabel?: string;
  maxHeight?: number;
}

function parseValue(value: unknown): { parsed: unknown; error: string | null } {
  if (typeof value !== "string") return { parsed: value, error: null };
  if (value.trim() === "") return { parsed: null, error: null };
  try {
    return { parsed: JSON.parse(value), error: null };
  } catch (error) {
    return { parsed: value, error: (error as Error).message };
  }
}

function stringify(parsed: unknown): string {
  if (parsed == null) return "";
  return JSON.stringify(parsed, null, 2);
}

export function JsonViewer({
  value,
  label,
  emptyLabel = "No data",
  maxHeight = 400,
}: JsonViewerProps) {
  const { parsed, error } = useMemo(() => parseValue(value), [value]);
  const text = useMemo(() => stringify(parsed), [parsed]);
  const isEmpty = text === "" || text === "null";

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        {label && (
          <span className="text-[11px] uppercase tracking-wide text-muted">
            {label}
          </span>
        )}
        {!isEmpty && <CopyButton value={text} size="sm" />}
      </div>
      {error && (
        <div className="text-[11px] text-accent-red">
          Could not parse JSON: {error}
        </div>
      )}
      {isEmpty ? (
        <div className="text-[12px] text-muted">{emptyLabel}</div>
      ) : (
        <pre
          className="overflow-x-auto overflow-y-auto border border-border bg-surface p-2 font-mono text-[11px] leading-relaxed"
          style={{ maxHeight }}
        >
          {text}
        </pre>
      )}
    </div>
  );
}
