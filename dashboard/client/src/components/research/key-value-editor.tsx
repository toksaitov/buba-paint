import { Plus, Trash2 } from "lucide-react";
import { Button, Input } from "../ui/dashboard-primitives";

export interface KeyValueRow {
  key: string;
  value: string;
}

interface KeyValueEditorProps {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  ariaLabel?: string;
}

export function KeyValueEditor({
  rows,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
  addLabel = "Add row",
  ariaLabel = "Key value editor",
}: KeyValueEditorProps) {
  const update = (index: number, patch: Partial<KeyValueRow>) => {
    onChange(
      rows.map((row, idx) =>
        idx === index ? { ...row, ...patch } : row,
      ),
    );
  };

  const remove = (index: number) => {
    onChange(rows.filter((_, idx) => idx !== index));
  };

  const add = () => {
    onChange([...rows, { key: "", value: "" }]);
  };

  return (
    <div className="space-y-2" role="group" aria-label={ariaLabel}>
      {rows.length === 0 && (
        <div className="text-[11px] text-muted">No rows yet.</div>
      )}
      {rows.map((row, index) => (
        <div
          key={index}
          className="flex flex-col gap-1.5 sm:flex-row sm:items-center"
        >
          <Input
            value={row.key}
            placeholder={keyPlaceholder}
            aria-label={`${ariaLabel} key ${index + 1}`}
            onChange={(event) =>
              update(index, { key: event.currentTarget.value })
            }
          />
          <Input
            value={row.value}
            placeholder={valuePlaceholder}
            aria-label={`${ariaLabel} value ${index + 1}`}
            onChange={(event) =>
              update(index, { value: event.currentTarget.value })
            }
          />
          <Button
            size="sm"
            tone="neutral"
            onClick={() => remove(index)}
            aria-label={`Remove row ${index + 1}`}
            iconLeft={<Trash2 size={12} />}
          >
            Remove
          </Button>
        </div>
      ))}
      <Button
        size="sm"
        tone="neutral"
        onClick={add}
        iconLeft={<Plus size={12} />}
      >
        {addLabel}
      </Button>
    </div>
  );
}
