import { Button, Input } from "../../ui/dashboard-primitives";
import type { KeyValueRow } from "../../../lib/research-types";
import { PARAMETER_OPTIONS, parameterOption } from "../job-create-params";

interface ParameterRowsEditorProps {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  addLabel: string;
  ariaLabel: string;
  valuePlaceholder: string;
}

export function ParameterRowsEditor({
  rows,
  onChange,
  addLabel,
  ariaLabel,
  valuePlaceholder,
}: ParameterRowsEditorProps) {
  const update = (index: number, patch: Partial<KeyValueRow>) => {
    onChange(
      rows.map((row, idx) => (idx === index ? { ...row, ...patch } : row)),
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
        <div className="text-[11px] text-muted">No parameters yet.</div>
      )}
      {rows.map((row, index) => {
        const option = parameterOption(row.key);
        const currentKeyKnown = row.key === "" || option != null;
        return (
          <div
            key={index}
            className="grid gap-2 border border-border bg-surface px-3 py-2 lg:grid-cols-[minmax(18rem,1.3fr)_minmax(14rem,1fr)_auto]"
          >
            <div className="min-w-0 space-y-1">
              <div className="text-[11px] font-semibold text-muted">
                Parameter
              </div>
              <select
                value={row.key}
                aria-label={`${ariaLabel} parameter ${index + 1}`}
                onChange={(event) =>
                  update(index, { key: event.currentTarget.value })
                }
                className="min-h-[40px] w-full border border-border bg-bg px-2 py-2 text-sm"
              >
                <option value="">Select parameter</option>
                {!currentKeyKnown && (
                  <option value={row.key}>{row.key}</option>
                )}
                {PARAMETER_OPTIONS.map((option) => (
                  <option key={option.key} value={option.key}>
                    {option.label}
                  </option>
                ))}
              </select>
              {(option?.hint || !currentKeyKnown) && (
                <div className="text-[11px] text-muted">
                  {option?.hint ?? "Custom parameter preserved from source job."}
                </div>
              )}
            </div>
            <div className="min-w-0 space-y-1">
              <div className="text-[11px] font-semibold text-muted">
                Value or range
              </div>
              <Input
                value={row.value}
                placeholder={option?.placeholder ?? valuePlaceholder}
                aria-label={`${ariaLabel} value ${index + 1}`}
                onChange={(event) =>
                  update(index, { value: event.currentTarget.value })
                }
              />
            </div>
            <Button
              size="sm"
              tone="neutral"
              onClick={() => remove(index)}
              aria-label={`Remove parameter ${index + 1}`}
              className="self-end"
            >
              Remove
            </Button>
          </div>
        );
      })}
      <Button size="sm" tone="neutral" onClick={add}>
        {addLabel}
      </Button>
    </div>
  );
}
