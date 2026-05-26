import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Plus } from "lucide-react";
import {
  Banner,
  Button,
  FormField,
  Input,
  ProgressBar,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { Dialog } from "../components/ui/dialog";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useResearchTransfers } from "../hooks/use-research-transfers";
import { useAuthStore } from "../stores/auth-store";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createArtifactTransfer } from "../lib/research-api";
import {
  checksumTone,
  progressTone,
  transferTone,
} from "../lib/research-permissions";
import type {
  CreateTransferRequest,
  TransferStatus,
} from "../lib/research-types";
import { formatBytes, humanize } from "../lib/utils";

const ALL_STATUSES: TransferStatus[] = [
  "queued",
  "running",
  "retryable",
  "paused",
  "failed",
  "cancelled",
  "completed",
];

const DEFAULT_FILTER: TransferStatus[] = [
  "queued",
  "running",
  "retryable",
  "paused",
  "failed",
];

export function ResearchTransfersPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const transfersQuery = useResearchTransfers();
  const artifactsQuery = useResearchArtifacts();
  const machinesQuery = useResearchMachines();
  const [active, setActive] = useState<string[]>([...DEFAULT_FILTER]);
  const [createOpen, setCreateOpen] = useState(false);

  const transfersData = transfersQuery.data?.transfers;
  const filtered = useMemo(
    () =>
      [...(transfersData ?? [])]
        .filter((t) => active.includes(t.status))
        .sort((a, b) => b.updated_at - a.updated_at),
    [transfersData, active],
  );

  if (transfersQuery.isLoading) {
    return <Loading label="Loading transfers" />;
  }
  if (transfersQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load transfers">
        {(transfersQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard
        title="All transfers"
        toolbar={
          <Button
            size="sm"
            tone="accent"
            iconLeft={<Plus size={14} />}
            onClick={() => setCreateOpen(true)}
            disabled={!isAdmin}
            title={isAdmin ? undefined : "Admin role required."}
          >
            New transfer
          </Button>
        }
      >
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => transferTone(s as TransferStatus)}
          ariaLabel="Transfer status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No transfers match the selected filters." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">ID</th>
                  <th className="px-2 py-1.5 font-semibold">Artifact</th>
                  <th className="px-2 py-1.5 font-semibold">Path</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">Progress</th>
                  <th className="px-2 py-1.5 font-semibold">Checksum</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((transfer) => {
                  const ratio =
                    transfer.bytes_total && transfer.bytes_total > 0
                      ? transfer.bytes_done / transfer.bytes_total
                      : 0;
                  return (
                    <tr
                      key={transfer.id}
                      className="border-b border-border last:border-b-0 hover:bg-surface"
                    >
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/transfers/${encodeURIComponent(transfer.id)}`}
                          className="font-mono text-[11px] hover:underline"
                        >
                          {transfer.id}
                        </Link>
                      </td>
                      <td className="px-2 py-1.5">
                        <Link
                          to={`/research/artifacts/${encodeURIComponent(transfer.artifact_id)}`}
                          className="font-mono text-[11px] hover:underline"
                        >
                          {transfer.artifact_id}
                        </Link>
                      </td>
                      <td className="px-2 py-1.5 text-muted">
                        <span className="font-mono text-[11px]">
                          {transfer.source_machine_id ?? "—"} →{" "}
                          {transfer.dest_machine_id ?? "—"}
                        </span>
                      </td>
                      <td className="px-2 py-1.5">
                        <StatusChip
                          label={humanize(transfer.status)}
                          tone={transferTone(transfer.status)}
                          compact
                        />
                      </td>
                      <td className="px-2 py-1.5">
                        <div className="flex flex-col gap-1 min-w-[140px]">
                          <ProgressBar
                            value={ratio}
                            ariaLabel={`Transfer progress ${(ratio * 100).toFixed(0)}%`}
                            tone={progressTone(transferTone(transfer.status))}
                          />
                          <span className="tabular-nums text-[11px] text-muted">
                            {formatBytes(transfer.bytes_done)} /{" "}
                            {formatBytes(transfer.bytes_total ?? undefined)}
                          </span>
                        </div>
                      </td>
                      <td className="px-2 py-1.5">
                        <StatusChip
                          label={
                            transfer.checksum_status
                              ? humanize(transfer.checksum_status)
                              : "—"
                          }
                          tone={checksumTone(transfer.checksum_status)}
                          compact
                        />
                      </td>
                      <td className="px-2 py-1.5 text-muted">
                        <RelativeTime epochMs={transfer.updated_at} />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
      <CreateTransferDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        artifacts={artifactsQuery.data?.artifacts ?? []}
        machines={machinesQuery.data?.machines ?? []}
      />
    </div>
  );
}

interface CreateTransferDialogProps {
  open: boolean;
  onClose: () => void;
  artifacts: { id: string; status: string }[];
  machines: { id: string; role: string }[];
}

function CreateTransferDialog({
  open,
  onClose,
  artifacts,
  machines,
}: CreateTransferDialogProps) {
  const queryClient = useQueryClient();
  const [artifactId, setArtifactId] = useState("");
  const [source, setSource] = useState("");
  const [dest, setDest] = useState("");
  const [bytesTotal, setBytesTotal] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (req: CreateTransferRequest) => createArtifactTransfer(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "transfers"] });
      setArtifactId("");
      setSource("");
      setDest("");
      setBytesTotal("");
      setError(null);
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const submit = () => {
    const bytes = bytesTotal.trim()
      ? Number(bytesTotal)
      : undefined;
    if (bytes != null && !Number.isFinite(bytes)) {
      setError("Bytes total must be a number.");
      return;
    }
    mutation.mutate({
      artifact_id: artifactId,
      source_machine_id: source || undefined,
      dest_machine_id: dest || undefined,
      bytes_total: bytes,
    });
  };

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Create transfer"
      description="Queues a transfer record. The destination worker claims it on its next tick."
    >
      <div className="space-y-3">
        <FormField label="Artifact" required>
          {({ id }) => (
            <select
              id={id}
              value={artifactId}
              onChange={(e) => setArtifactId(e.currentTarget.value)}
              className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
            >
              <option value="">Select artifact</option>
              {artifacts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.id}
                </option>
              ))}
            </select>
          )}
        </FormField>
        <div className="grid gap-3 sm:grid-cols-2">
          <FormField label="Source machine">
            {({ id }) => (
              <select
                id={id}
                value={source}
                onChange={(e) => setSource(e.currentTarget.value)}
                className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
              >
                <option value="">—</option>
                {machines.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.id}
                  </option>
                ))}
              </select>
            )}
          </FormField>
          <FormField label="Destination machine">
            {({ id }) => (
              <select
                id={id}
                value={dest}
                onChange={(e) => setDest(e.currentTarget.value)}
                className="w-full border border-border bg-bg px-2 py-1.5 text-sm"
              >
                <option value="">—</option>
                {machines.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.id}
                  </option>
                ))}
              </select>
            )}
          </FormField>
        </div>
        <FormField label="Bytes total" hint="Optional">
          {({ id }) => (
            <Input
              id={id}
              inputMode="numeric"
              value={bytesTotal}
              onChange={(e) => setBytesTotal(e.currentTarget.value)}
            />
          )}
        </FormField>
        {error && (
          <Banner tone="danger" title="Could not create transfer">
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
            disabled={!artifactId || mutation.isPending}
            state={mutation.isPending ? "pending" : "idle"}
          >
            Create transfer
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
