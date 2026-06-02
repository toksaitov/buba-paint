import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  Button,
  KeyValueList,
  ProgressBar,
  RelativeTime,
  SectionCard,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { MachineReference } from "../components/research/machine-reference";
import { StaleTransferBanner } from "../components/research/stale-transfer-banner";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useResearchTransfer } from "../hooks/use-research-transfers";
import { useAuthStore } from "../stores/auth-store";
import {
  cancelArtifactTransfer,
  deleteArtifactTransfer,
  pauseArtifactTransfer,
  resumeArtifactTransfer,
  retryArtifactTransfer,
  verifyArtifactTransfer,
} from "../lib/research-api";
import {
  ACTION_LABELS,
  checksumTone,
  getActionGateState,
  getAllowedActions,
  isTransferTerminal,
  permissionHint,
  progressTone,
  transferTone,
} from "../lib/research-permissions";
import type {
  ArtifactVerification,
  ResearchAction,
} from "../lib/research-types";
import { formatBytes, formatDateTime, humanize } from "../lib/utils";

export function ResearchTransferDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const returnToTransfers = useResearchReturnTo(
    "transfers",
    "/research/transfers",
  );
  const role = user?.role;

  const transferQuery = useResearchTransfer(id);
  const machinesQuery = useResearchMachines();
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<ResearchAction | null>(
    null,
  );
  const [verifyResult, setVerifyResult] = useState<
    | { ok: true; verification: ArtifactVerification }
    | { ok: false; message: string }
    | null
  >(null);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "transfers"] });
    queryClient.invalidateQueries({
      queryKey: ["research", "transfer", id],
    });
  };

  const runMutation = (
    action: ResearchAction,
    fn: () => Promise<unknown>,
    extraOnSuccess?: () => void,
  ) => {
    setPendingAction(action);
    setActionError(null);
    fn()
      .then(() => {
        setPendingAction(null);
        invalidate();
        if (extraOnSuccess) extraOnSuccess();
      })
      .catch((err: Error) => {
        setPendingAction(null);
        setActionError(err.message);
      });
  };

  const deleteMutation = useMutation({
    mutationFn: () => deleteArtifactTransfer(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "transfers"] });
      navigate(returnToTransfers);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const verifyMutation = useMutation({
    mutationFn: () => verifyArtifactTransfer(id),
    onMutate: () => {
      setPendingAction("verify");
      setActionError(null);
      setVerifyResult(null);
    },
    onSuccess: (res) => {
      setPendingAction(null);
      setVerifyResult({ ok: true, verification: res.verification });
      invalidate();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setVerifyResult({ ok: false, message: err.message });
    },
  });

  if (transferQuery.isLoading) {
    return <Loading label="Loading transfer" />;
  }
  if (transferQuery.isError || !transferQuery.data) {
    return (
      <Banner tone="danger" title="Could not load transfer">
        {(transferQuery.error as Error)?.message ?? "Transfer not found."}
      </Banner>
    );
  }

  const transfer = transferQuery.data;
  const isActive = !isTransferTerminal(transfer.status);
  const allowed = getAllowedActions("transfer", transfer.status);
  const ratio =
    transfer.bytes_total && transfer.bytes_total > 0
      ? transfer.bytes_done / transfer.bytes_total
      : 0;

  const handleAction = (action: ResearchAction) => {
    switch (action) {
      case "pause":
        return runMutation("pause", () => pauseArtifactTransfer(id));
      case "resume":
        return runMutation("resume", () => resumeArtifactTransfer(id));
      case "cancel":
        return runMutation("cancel", () => cancelArtifactTransfer(id));
      case "retry":
        return runMutation("retry", () =>
          retryArtifactTransfer(id, { resume: true }),
        );
      case "verify":
        return verifyMutation.mutate();
      case "delete":
        return setConfirmDeleteOpen(true);
      default:
        return undefined;
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToTransfers}
          className="text-[12px] text-muted hover:underline"
        >
          ← Transfers
        </Link>
        <span className="font-mono text-[12px]">{transfer.id}</span>
        <StatusChip
          label={humanize(transfer.status)}
          tone={transferTone(transfer.status)}
        />
        <StatusChip
          label={`Checksum: ${transfer.checksum_status ? humanize(transfer.checksum_status) : "-"}`}
          tone={checksumTone(transfer.checksum_status)}
          compact
        />
      </div>

      <StaleTransferBanner transfer={transfer} />

      {isActive && (
        <Banner tone="info" title="Durable state changes immediately">
          Pause and cancel update the transfer record now. A running rsync may
          continue briefly until the worker observes the new state.
        </Banner>
      )}

      {actionError && (
        <Banner tone="danger" title="Action failed">
          {actionError}
        </Banner>
      )}
      {verifyResult && !verifyResult.ok && (
        <Banner tone="danger" title="Verification failed">
          {verifyResult.message}
        </Banner>
      )}
      {verifyResult && verifyResult.ok && (
        <Banner tone="success" title="Verification succeeded">
          {verifyResult.verification.files_checked} files,{" "}
          {formatBytes(verifyResult.verification.bytes_checked)} checked.
        </Banner>
      )}

      <SectionCard title="Progress">
        <div className="space-y-3">
          <div className="space-y-2">
            <ProgressBar
              value={ratio}
              ariaLabel={`Transfer progress ${(ratio * 100).toFixed(0)}%`}
              tone={progressTone(transferTone(transfer.status))}
            />
            <div className="flex flex-wrap items-center justify-between gap-2 text-[12px]">
              <span className="tabular-nums">
                {formatBytes(transfer.bytes_done)} /{" "}
                {formatBytes(transfer.bytes_total ?? undefined)}
              </span>
              <span className="tabular-nums text-muted">
                {(ratio * 100).toFixed(1)}%
              </span>
            </div>
          </div>
          <KeyValueList
            columns={2}
            items={[
              {
                label: "Last update",
                value: <RelativeTime epochMs={transfer.updated_at} />,
              },
              {
                label: "Created",
                value: <RelativeTime epochMs={transfer.created_at} />,
              },
              {
                label: "Completed",
                value: transfer.completed_at ? (
                  <RelativeTime epochMs={transfer.completed_at} />
                ) : (
                  "-"
                ),
              },
              {
                label: "Updated at (absolute)",
                value: formatDateTime(transfer.updated_at),
              },
            ]}
          />
        </div>
      </SectionCard>

      <SectionCard title="Endpoints">
        <KeyValueList
          columns={2}
          items={[
            {
              label: "Artifact",
              value: (
                <Link
                  to={`/research/artifacts/${encodeURIComponent(transfer.artifact_id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {transfer.artifact_id}
                </Link>
              ),
            },
            {
              label: "Source machine",
              value: (
                <MachineReference
                  machineId={transfer.source_machine_id}
                  machines={machinesQuery.data?.machines ?? []}
                />
              ),
            },
            {
              label: "Destination machine",
              value: (
                <MachineReference
                  machineId={transfer.dest_machine_id}
                  machines={machinesQuery.data?.machines ?? []}
                />
              ),
            },
            {
              label: "Source job",
              value: (
                <span className="text-muted">
                  Not linked. Jobs and transfers are not joined in the current
                  backend schema.
                </span>
              ),
            },
          ]}
        />
      </SectionCard>

      {transfer.error && (
        <SectionCard title="Error">
          <Banner tone="danger" title="Worker reported an error">
            <pre className="whitespace-pre-wrap font-mono text-[11px]">
              {transfer.error}
            </pre>
          </Banner>
        </SectionCard>
      )}

      <SectionCard title="Actions">
        <div className="flex flex-wrap items-center gap-2">
          {allowed.map((action) => {
            const gate = getActionGateState(role, action);
            const disabled = gate !== "enabled";
            const isPending = pendingAction === action || (action === "delete" && deleteMutation.isPending);
            const tone =
              action === "delete" || action === "cancel" ? "danger" : "neutral";
            return (
              <Button
                key={action}
                size="sm"
                tone={tone}
                disabled={disabled || isPending}
                state={isPending ? "pending" : "idle"}
                title={disabled ? permissionHint(action) : undefined}
                onClick={() => handleAction(action)}
              >
                {ACTION_LABELS[action]}
              </Button>
            );
          })}
        </div>
      </SectionCard>

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="Delete transfer record"
        description="Removes only this transfer metadata record. Artifact files and reports are not touched."
        confirmLabel="Delete record"
        destructive
        pending={deleteMutation.isPending}
        errorMessage={
          deleteMutation.isError
            ? (deleteMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteMutation.mutate()}
        onClose={() => setConfirmDeleteOpen(false)}
      />
    </div>
  );
}
