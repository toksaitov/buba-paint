import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Plus } from "lucide-react";
import {
  Banner,
  Button,
  FormField,
  Input,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { Dialog } from "../components/ui/dialog";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchJobs } from "../hooks/use-research-jobs";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useResearchReports } from "../hooks/use-research-reports";
import { useResearchTransfers } from "../hooks/use-research-transfers";
import { useAuthStore } from "../stores/auth-store";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  importResearchArtifact,
  registerResearchArtifact,
} from "../lib/research-api";
import {
  artifactTone,
  isJobTerminal,
  isTransferTerminal,
} from "../lib/research-permissions";
import type {
  ArtifactStatus,
  ImportArtifactRequest,
  JobStatus,
  RegisterArtifactRequest,
  TransferStatus,
} from "../lib/research-types";
import { formatBytes, formatDateTime, humanize } from "../lib/utils";

const ALL_STATUSES: ArtifactStatus[] = ["available", "archived"];

const PRESET_OPTIONS = [
  { value: "all", label: "All artifacts" },
  { value: "available", label: "Available" },
  { value: "archived", label: "Archived" },
  { value: "active_dependencies", label: "Active dependencies" },
  { value: "archive_eligible", label: "Archive eligible" },
  { value: "missing_readiness", label: "Missing readiness" },
] as const;

type ArtifactPreset = (typeof PRESET_OPTIONS)[number]["value"];

export function ResearchArtifactsPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const artifactsQuery = useResearchArtifacts();
  const machinesQuery = useResearchMachines();
  const jobsQuery = useResearchJobs();
  const transfersQuery = useResearchTransfers();
  const reportsQuery = useResearchReports();
  const [active, setActive] = useState<string[]>(["available"]);
  const [importOpen, setImportOpen] = useState(false);
  const [registerOpen, setRegisterOpen] = useState(false);
  const [preset, setPreset] = useState<ArtifactPreset>("available");
  const [search, setSearch] = useState("");

  const artifactsData = artifactsQuery.data?.artifacts;
  const machines = machinesQuery.data?.machines ?? [];
  const jobsData = jobsQuery.data?.jobs;
  const transfersData = transfersQuery.data?.transfers;
  const reportsData = reportsQuery.data?.reports;
  const filtered = useMemo(
    () => {
      const needle = search.trim().toLowerCase();
      const jobs = jobsData ?? [];
      const transfers = transfersData ?? [];
      const reports = reportsData ?? [];
      return (artifactsData ?? [])
        .filter((a) => active.includes(a.status))
        .filter((a) =>
          artifactPresetMatches(preset, a, jobs, transfers, reports),
        )
        .filter((a) => {
          if (!needle) return true;
          return [
            a.id,
            a.kind,
            a.status,
            a.run_mode ?? "",
            a.source_machine_id ?? "",
            a.checksum ?? "",
          ]
            .join(" ")
            .toLowerCase()
            .includes(needle);
        });
    },
    [artifactsData, active, preset, jobsData, transfersData, reportsData, search],
  );

  if (artifactsQuery.isLoading) {
    return <Loading label="Loading artifacts" />;
  }
  if (artifactsQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load artifacts">
        {(artifactsQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard
        title="All artifacts"
        toolbar={
          <div className="flex gap-2">
            <select
              aria-label="Artifact preset"
              value={preset}
              onChange={(event) =>
                setPreset(event.currentTarget.value as ArtifactPreset)
              }
              className="border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {PRESET_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
            <Button
              size="sm"
              iconLeft={<Plus size={14} />}
              onClick={() => setImportOpen(true)}
              disabled={!isAdmin}
              title={isAdmin ? undefined : "Admin role required."}
            >
              Import local
            </Button>
            <Button
              size="sm"
              iconLeft={<Plus size={14} />}
              onClick={() => setRegisterOpen(true)}
              disabled={!isAdmin}
              title={isAdmin ? undefined : "Admin role required."}
            >
              Register remote
            </Button>
          </div>
        }
      >
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => artifactTone(s as ArtifactStatus)}
          ariaLabel="Artifact status filter"
        />
        <div className="mb-3 max-w-md">
          <Input
            aria-label="Search artifacts"
            value={search}
            onChange={(event) => setSearch(event.currentTarget.value)}
            placeholder="Search artifact, mode, source"
          />
        </div>
        {filtered.length === 0 ? (
          <StateEmpty message="No artifacts match the selected filters. Create an export job or import an artifact directory to populate this list." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">ID</th>
                  <th className="px-2 py-1.5 font-semibold">Kind</th>
                  <th className="px-2 py-1.5 font-semibold">Run mode</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">Bytes</th>
                  <th className="px-2 py-1.5 font-semibold">Source</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((artifact) => (
                  <tr
                    key={artifact.id}
                    className="border-b border-border last:border-b-0 hover:bg-surface"
                  >
                    <td className="px-2 py-1.5">
                      <Link
                        to={`/research/artifacts/${encodeURIComponent(artifact.id)}`}
                        className="font-mono text-[11px] hover:underline"
                      >
                        {artifact.id}
                      </Link>
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      {humanize(artifact.kind)}
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      {artifact.run_mode ? humanize(artifact.run_mode) : "—"}
                    </td>
                    <td className="px-2 py-1.5">
                      <StatusChip
                        label={humanize(artifact.status)}
                        tone={artifactTone(artifact.status)}
                        compact
                      />
                    </td>
                    <td className="px-2 py-1.5 tabular-nums">
                      {formatBytes(artifact.bytes ?? undefined)}
                    </td>
                    <td className="px-2 py-1.5 font-mono text-[11px] text-muted">
                      {artifact.source_machine_id ?? "—"}
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      <RelativeTime epochMs={artifact.updated_at} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
      <ImportArtifactDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        machines={machines.map((m) => m.id)}
      />
      <RegisterArtifactDialog
        open={registerOpen}
        onClose={() => setRegisterOpen(false)}
        machines={machines.map((m) => m.id)}
      />
      <div className="text-[11px] text-muted">
        Last refreshed{" "}
        {artifactsQuery.dataUpdatedAt
          ? formatDateTime(artifactsQuery.dataUpdatedAt)
          : "—"}
      </div>
    </div>
  );
}

function artifactPresetMatches(
  preset: ArtifactPreset,
  artifact: {
    id: string;
    status: ArtifactStatus;
    replay_quality_class: string | null;
    backtest_ready_class: string | null;
    live_fidelity_class: string | null;
  },
  jobs: { artifact_id: string | null; status: string }[],
  transfers: { artifact_id: string; status: string }[],
  reports: { artifact_id: string | null }[],
): boolean {
  const activeDependencies = activeArtifactDependencies(
    artifact.id,
    jobs,
    transfers,
  );
  if (preset === "all") return true;
  if (preset === "available") return artifact.status === "available";
  if (preset === "archived") return artifact.status === "archived";
  if (preset === "active_dependencies") return activeDependencies > 0;
  if (preset === "archive_eligible") {
    return artifact.status === "available" && activeDependencies === 0;
  }
  return (
    artifact.replay_quality_class == null ||
    artifact.backtest_ready_class == null ||
    artifact.live_fidelity_class == null ||
    !reports.some((report) => report.artifact_id === artifact.id)
  );
}

function activeArtifactDependencies(
  artifactId: string,
  jobs: { artifact_id: string | null; status: string }[],
  transfers: { artifact_id: string; status: string }[],
): number {
  return (
    jobs.filter(
      (job) =>
        job.artifact_id === artifactId &&
        !isJobTerminal(job.status as JobStatus),
    ).length +
    transfers.filter(
      (transfer) =>
        transfer.artifact_id === artifactId &&
        !isTransferTerminal(transfer.status as TransferStatus),
    ).length
  );
}

interface ImportArtifactDialogProps {
  open: boolean;
  onClose: () => void;
  machines: string[];
}

function ImportArtifactDialog({
  open,
  onClose,
  machines,
}: ImportArtifactDialogProps) {
  const queryClient = useQueryClient();
  const [root, setRoot] = useState("");
  const [id, setId] = useState("");
  const [sourceMachineId, setSourceMachineId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (req: ImportArtifactRequest) => importResearchArtifact(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
      setRoot("");
      setId("");
      setSourceMachineId("");
      setError(null);
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Import local artifact"
      description="Verify and register an artifact directory that already lives under the research work root."
    >
      <div className="space-y-3">
        <FormField label="Artifact root" required>
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={root}
              onChange={(e) => setRoot(e.currentTarget.value)}
              placeholder="/research/artifacts/my-artifact"
            />
          )}
        </FormField>
        <FormField label="Artifact ID override" hint="Optional">
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={id}
              onChange={(e) => setId(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Source machine ID" hint="Optional">
          {({ id: fieldId }) => (
            <select
              id={fieldId}
              value={sourceMachineId}
              onChange={(e) => setSourceMachineId(e.currentTarget.value)}
              className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
            >
              <option value="">—</option>
              {machines.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          )}
        </FormField>
        {error && (
          <Banner tone="danger" title="Import failed">
            {error}
          </Banner>
        )}
        <div className="flex justify-end gap-2">
          <Button onClick={onClose} disabled={mutation.isPending}>
            Cancel
          </Button>
          <Button
            tone="accent"
            onClick={() =>
              mutation.mutate({
                artifact_root: root.trim(),
                artifact_id: id.trim() || undefined,
                source_machine_id: sourceMachineId || undefined,
              })
            }
            disabled={!root.trim() || mutation.isPending}
            state={mutation.isPending ? "pending" : "idle"}
          >
            Import
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

interface RegisterArtifactDialogProps {
  open: boolean;
  onClose: () => void;
  machines: string[];
}

function RegisterArtifactDialog({
  open,
  onClose,
  machines,
}: RegisterArtifactDialogProps) {
  const queryClient = useQueryClient();
  const [root, setRoot] = useState("");
  const [manifestText, setManifestText] = useState("");
  const [sourceMachineId, setSourceMachineId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (req: RegisterArtifactRequest) => registerResearchArtifact(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
      setRoot("");
      setManifestText("");
      setSourceMachineId("");
      setError(null);
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const submit = () => {
    setError(null);
    let manifest: RegisterArtifactRequest["manifest"];
    try {
      manifest = JSON.parse(manifestText);
    } catch (err) {
      setError(`Manifest JSON is invalid: ${(err as Error).message}`);
      return;
    }
    mutation.mutate({
      artifact_root: root.trim(),
      manifest,
      source_machine_id: sourceMachineId || undefined,
    });
  };

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Register remote artifact"
      description="Record manifest metadata for an artifact that lives on another machine. Files are verified after transfer."
      width="lg"
    >
      <div className="space-y-3">
        <FormField label="Remote artifact root" required>
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={root}
              onChange={(e) => setRoot(e.currentTarget.value)}
              placeholder="/absolute/path/on/remote/host"
            />
          )}
        </FormField>
        <FormField label="Manifest JSON" required>
          {({ id: fieldId }) => (
            <Textarea
              id={fieldId}
              value={manifestText}
              onChange={(e) => setManifestText(e.currentTarget.value)}
              minRows={8}
              placeholder='{"schema_version":1,"artifact_id":"..."}'
            />
          )}
        </FormField>
        <FormField label="Source machine" required>
          {({ id: fieldId }) => (
            <select
              id={fieldId}
              value={sourceMachineId}
              onChange={(e) => setSourceMachineId(e.currentTarget.value)}
              className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
            >
              <option value="">—</option>
              {machines.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          )}
        </FormField>
        {error && (
          <Banner tone="danger" title="Register failed">
            {error}
          </Banner>
        )}
        <div className="flex justify-end gap-2">
          <Button onClick={onClose} disabled={mutation.isPending}>
            Cancel
          </Button>
          <Button
            tone="accent"
            onClick={submit}
            disabled={!root.trim() || !manifestText.trim() || mutation.isPending}
            state={mutation.isPending ? "pending" : "idle"}
          >
            Register
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
