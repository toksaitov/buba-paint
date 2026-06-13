import { FormField, Segment } from "../../ui/dashboard-primitives";
import {
  DEFAULT_SWEEP_ROWS,
  defaultParameterRows,
  parameterOption,
  sweepCombinationCount,
  sweepRowsForState,
  type SweepScope,
  type SweepState,
} from "../job-create-params";
import { ParameterRowsEditor } from "./parameter-rows-editor";

interface SweepFieldsProps {
  state: SweepState;
  onChange: (next: SweepState) => void;
}

export function SweepFields({ state, onChange }: SweepFieldsProps) {
  const activeRows = sweepRowsForState(state);
  const combinations = sweepCombinationCount(activeRows);
  return (
    <div className="space-y-3">
      <FormField label="Sweep breadth">
        {() => (
          <div className="space-y-2">
            <Segment
              value={state.sweep_scope}
              onChange={(value) =>
                onChange({
                  ...state,
                  sweep_scope: value as SweepScope,
                  sweeps:
                    value === "full"
                      ? DEFAULT_SWEEP_ROWS
                      : defaultParameterRows(
                          state.sweeps.length > 0
                            ? state.sweeps
                            : DEFAULT_SWEEP_ROWS.slice(0, 1),
                        ),
                })
              }
              items={[
                { value: "full", label: "Full sweep preset" },
                { value: "focused", label: "Focused ranges" },
              ]}
              ariaLabel="Sweep breadth"
            />
            <div className="text-[12px] text-muted">
              {state.sweep_scope === "full"
                ? "Tests the main latency-arbitrage thresholds and simulated order latency across the default grid."
                : "Tests only the ranges selected below."}
            </div>
          </div>
        )}
      </FormField>
      {state.sweep_scope === "focused" && (
        <FormField
          label="Focused parameter ranges"
          hint="Each value is a comma-separated range, for example 0.30,0.35,0.40."
          required
        >
          {() => (
            <ParameterRowsEditor
              rows={state.sweeps}
              onChange={(rows) => onChange({ ...state, sweeps: rows })}
              addLabel="Add parameter range"
              ariaLabel="Sweep dimensions"
              valuePlaceholder="0.30,0.35,0.40"
            />
          )}
        </FormField>
      )}
      {state.sweep_scope === "full" && (
        <div className="border border-border bg-bg px-3 py-2 text-[12px]">
          <div className="font-semibold">Full sweep preset</div>
          <ul className="mt-2 space-y-1">
            {DEFAULT_SWEEP_ROWS.map((row) => (
              <li key={row.key}>
                <span className="text-muted">
                  {parameterOption(row.key)?.label ?? row.key}:
                </span>{" "}
                <span className="font-mono">{row.value}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="border border-border bg-bg px-3 py-2 text-[12px]">
        <div className="font-semibold">Sweep size</div>
        <div className="mt-1 text-muted">
          {activeRows.length === 0
            ? "Add at least one parameter range."
            : `${combinations} parameter combination${
                combinations === 1 ? "" : "s"
              } will run.`}
        </div>
      </div>
    </div>
  );
}
