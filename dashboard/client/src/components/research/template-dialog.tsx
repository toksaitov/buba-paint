import { useState } from "react";
import {
  Banner,
  Button,
  FormField,
  Input,
  Textarea,
} from "../ui/dashboard-primitives";
import { Dialog } from "../ui/dialog";
import type {
  ResearchJobTemplate,
  UpsertJobTemplateRequest,
} from "../../lib/research-types";

export type TemplateDialogMode = "create" | "edit";

export function TemplateDialog({
  open,
  mode,
  template,
  artifacts,
  pending,
  error,
  onSubmit,
  onClose,
}: {
  open: boolean;
  mode: TemplateDialogMode;
  template: ResearchJobTemplate | null;
  artifacts: { id: string; status: string }[];
  pending: boolean;
  error: string | null;
  onSubmit: (req: UpsertJobTemplateRequest) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(template?.name ?? "");
  const [description, setDescription] = useState(template?.description ?? "");
  const [jobType, setJobType] = useState<"current_params" | "sweep">(
    template?.job_type ?? "current_params",
  );
  const [artifactId, setArtifactId] = useState(template?.artifact_id ?? "");
  const [priority, setPriority] = useState(String(template?.priority ?? 0));
  const [paramsJson, setParamsJson] = useState(template?.params_json ?? "{}");
  const params = parseParams(paramsJson);
  const priorityNumber = Number(priority);
  const canSubmit =
    name.trim().length > 0 &&
    params.error == null &&
    Number.isInteger(priorityNumber) &&
    !pending;

  return (
    <Dialog
      open={open}
      onClose={pending ? () => undefined : onClose}
      title={mode === "edit" ? "Edit template" : "Create template"}
      description="Templates are shared defaults for backtest and sweep jobs. The easiest way to build one is to configure a job on the New Job page and click Save as template there."
      width="lg"
    >
      <div className="space-y-3">
        {error && (
          <Banner tone="danger" title="Template save failed">
            {error}
          </Banner>
        )}
        <FormField label="Name" required>
          {({ id }) => (
            <Input
              id={id}
              value={name}
              onChange={(event) => setName(event.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Description" hint="Optional">
          {({ id }) => (
            <Textarea
              id={id}
              value={description}
              onChange={(event) => setDescription(event.currentTarget.value)}
              minRows={3}
            />
          )}
        </FormField>
        <div className="grid gap-3 sm:grid-cols-3">
          <FormField label="Job type">
            {({ id }) => (
              <select
                id={id}
                value={jobType}
                onChange={(event) =>
                  setJobType(
                    event.currentTarget.value as "current_params" | "sweep",
                  )
                }
                className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
              >
                <option value="current_params">Backtest</option>
                <option value="sweep">Sweep</option>
              </select>
            )}
          </FormField>
          <FormField label="Artifact" hint="Optional">
            {({ id }) => (
              <select
                id={id}
                value={artifactId}
                onChange={(event) => setArtifactId(event.currentTarget.value)}
                className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
              >
                <option value="">None</option>
                {artifacts
                  .filter((artifact) => artifact.status === "available")
                  .map((artifact) => (
                    <option key={artifact.id} value={artifact.id}>
                      {artifact.id}
                    </option>
                  ))}
              </select>
            )}
          </FormField>
          <FormField label="Priority">
            {({ id }) => (
              <Input
                id={id}
                value={priority}
                inputMode="numeric"
                onChange={(event) => setPriority(event.currentTarget.value)}
              />
            )}
          </FormField>
        </div>
        <FormField label="Params JSON" required>
          {({ id }) => (
            <div className="space-y-1">
              <Textarea
                id={id}
                value={paramsJson}
                onChange={(event) => setParamsJson(event.currentTarget.value)}
                minRows={8}
              />
              {params.error && (
                <div className="text-[12px] text-accent-red">
                  {params.error}
                </div>
              )}
            </div>
          )}
        </FormField>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose} disabled={pending}>
            Cancel
          </Button>
          <Button
            tone="accent"
            disabled={!canSubmit}
            state={pending ? "pending" : "idle"}
            onClick={() =>
              onSubmit({
                name: name.trim(),
                description: description.trim() || undefined,
                job_type: jobType,
                artifact_id: artifactId || undefined,
                priority: priorityNumber,
                params: params.value,
              })
            }
          >
            Save template
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

function parseParams(value: string): {
  value: Record<string, unknown>;
  error: string | null;
} {
  try {
    const parsed: unknown = JSON.parse(value);
    if (
      parsed == null ||
      typeof parsed !== "object" ||
      Array.isArray(parsed)
    ) {
      return { value: {}, error: "Params must be a JSON object." };
    }
    return { value: parsed as Record<string, unknown>, error: null };
  } catch (error) {
    return {
      value: {},
      error: error instanceof Error ? error.message : "Invalid JSON.",
    };
  }
}
