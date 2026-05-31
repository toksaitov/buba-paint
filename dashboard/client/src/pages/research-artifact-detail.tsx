import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  Button,
  FormField,
  InfoHint,
  Input,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { Dialog } from "../components/ui/dialog";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { JsonViewer } from "../components/research/json-viewer";
import { MachineReference } from "../components/research/machine-reference";
import {
  useResearchArtifact,
  useResearchArtifactChecksums,
  useResearchArtifactManifest,
} from "../hooks/use-research-artifacts";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useResearchTransfers } from "../hooks/use-research-transfers";
import { useResearchJobs } from "../hooks/use-research-jobs";
import { useResearchReports } from "../hooks/use-research-reports";
import { useResearchReturnTo } from "../hooks/use-research-return-to";
import { useAuthStore } from "../stores/auth-store";
import {
  archiveResearchArtifact,
  deleteResearchArtifact,
  restoreResearchArtifact,
  updateResearchArtifact,
  verifyResearchArtifact,
} from "../lib/research-api";
import {
  ACTION_LABELS,
  artifactTone,
  canPerform,
  checksumTone,
  getActionGateState,
  getAllowedActions,
  permissionHint,
} from "../lib/research-permissions";
import type {
  ArtifactVerification,
  ResearchAction,
  UpdateArtifactRequest,
} from "../lib/research-types";
import { formatBytes, formatDateTime, humanize } from "../lib/utils";

export function ResearchArtifactDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;
  const returnToArtifacts = useResearchReturnTo(
    "artifacts",
    "/research/artifacts",
  );

  const artifactQuery = useResearchArtifact(id);
  const [showManifest, setShowManifest] = useState(false);
  const [showChecksums, setShowChecksums] = useState(false);
  const manifestQuery = useResearchArtifactManifest(id, showManifest);
  const checksumsQuery = useResearchArtifactChecksums(id, showChecksums);

  const transfersQuery = useResearchTransfers();
  const jobsQuery = useResearchJobs();
  const reportsQuery = useResearchReports();
  const machinesQuery = useResearchMachines();

  const [editOpen, setEditOpen] = useState(false);
  const [verifyResult, setVerifyResult] = useState<
    | { ok: true; verification: ArtifactVerification }
    | { ok: false; message: string }
    | null
  >(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<ResearchAction | null>(
    null,
  );
  const [confirmDeleteRecordOpen, setConfirmDeleteRecordOpen] =
    useState(false);
  const [confirmDeleteFilesOpen, setConfirmDeleteFilesOpen] = useState(false);

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
    queryClient.invalidateQueries({
      queryKey: ["research", "artifact", id],
    });
  };

  const verifyMutation = useMutation({
    mutationFn: () => verifyResearchArtifact(id),
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

  const archiveMutation = useMutation({
    mutationFn: () => archiveResearchArtifact(id),
    onMutate: () => {
      setPendingAction("archive");
      setActionError(null);
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidate();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const restoreMutation = useMutation({
    mutationFn: () => restoreResearchArtifact(id),
    onMutate: () => {
      setPendingAction("restore");
      setActionError(null);
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidate();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const deleteRecordMutation = useMutation({
    mutationFn: () => deleteResearchArtifact(id, false),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
      navigate(returnToArtifacts);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const deleteFilesMutation = useMutation({
    mutationFn: () => deleteResearchArtifact(id, true),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
      navigate(returnToArtifacts);
    },
    onError: (err: Error) => setActionError(err.message),
  });

  if (artifactQuery.isLoading) {
    return <Loading label="Loading artifact" />;
  }
  if (artifactQuery.isError || !artifactQuery.data) {
    return (
      <Banner tone="danger" title="Could not load artifact">
        {(artifactQuery.error as Error)?.message ?? "Artifact not found."}
      </Banner>
    );
  }

  const artifact = artifactQuery.data;
  const allowed = getAllowedActions("artifact", artifact.status);

  const linkedTransfers = (transfersQuery.data?.transfers ?? []).filter(
    (t) => t.artifact_id === artifact.id,
  );
  const linkedJobs = (jobsQuery.data?.jobs ?? []).filter(
    (j) => j.artifact_id === artifact.id,
  );
  const linkedReports = (reportsQuery.data?.reports ?? []).filter(
    (r) => r.artifact_id === artifact.id,
  );

  const intervalLabel =
    artifact.interval_start_ms && artifact.interval_end_ms
      ? `${formatDateTime(artifact.interval_start_ms)} → ${formatDateTime(artifact.interval_end_ms)}`
      : "—";

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to={returnToArtifacts}
          className="text-[12px] text-muted hover:underline"
        >
          ← Artifacts
        </Link>
        <span className="font-mono text-[12px]">{artifact.id}</span>
        <StatusChip
          label={humanize(artifact.status)}
          tone={artifactTone(artifact.status)}
        />
      </div>

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
      {actionError && (
        <Banner tone="danger" title="Action failed">
          {actionError}
        </Banner>
      )}

      <SectionCard title="Metadata">
        <KeyValueList
          columns={2}
          items={[
            { label: "Kind", value: humanize(artifact.kind) },
            {
              label: "Run mode",
              value: artifact.run_mode ? humanize(artifact.run_mode) : "—",
            },
            {
              label: "Source machine",
              value: (
                <MachineReference
                  machineId={artifact.source_machine_id}
                  machines={machinesQuery.data?.machines ?? []}
                />
              ),
            },
            { label: "Interval", value: intervalLabel },
            { label: "Bytes", value: formatBytes(artifact.bytes ?? undefined) },
            {
              label: "Checksum",
              value: artifact.checksum ? (
                <span className="font-mono text-[11px]">
                  {artifact.checksum.slice(0, 16)}…
                </span>
              ) : (
                <StatusChip
                  label={"none"}
                  tone={checksumTone(null)}
                  compact
                />
              ),
            },
            {
              label: "Replay quality",
              value: artifact.replay_quality_class
                ? humanize(artifact.replay_quality_class)
                : "—",
            },
            {
              label: "Backtest readiness",
              value: artifact.backtest_ready_class
                ? humanize(artifact.backtest_ready_class)
                : "—",
            },
            {
              label: "Live fidelity",
              value: artifact.live_fidelity_class
                ? humanize(artifact.live_fidelity_class)
                : "—",
            },
            {
              label: "Manifest path",
              value: artifact.manifest_path ? (
                <span className="font-mono text-[11px]">
                  {artifact.manifest_path}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: "Bundle path",
              value: artifact.bundle_path ? (
                <span className="font-mono text-[11px]">
                  {artifact.bundle_path}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: "Source DB path",
              value: artifact.source_db_path ? (
                <span className="font-mono text-[11px]">
                  {artifact.source_db_path}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: "Created",
              value: <RelativeTime epochMs={artifact.created_at} />,
            },
            {
              label: "Updated",
              value: <RelativeTime epochMs={artifact.updated_at} />,
            },
            {
              label: "Archived",
              value: artifact.archived_at ? (
                <RelativeTime epochMs={artifact.archived_at} />
              ) : (
                "—"
              ),
            },
          ]}
        />
      </SectionCard>

      <SectionCard
        title="Manifest"
        toolbar={
          <Button size="sm" onClick={() => setShowManifest((s) => !s)}>
            {showManifest ? "Hide" : "Load manifest"}
          </Button>
        }
      >
        {!showManifest ? (
          <StateEmpty message="Click 'Load manifest' to fetch and inspect the artifact manifest." />
        ) : manifestQuery.isLoading ? (
          <Loading label="Loading manifest" />
        ) : manifestQuery.isError ? (
          <Banner tone="danger" title="Could not load manifest">
            {(manifestQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : manifestQuery.data ? (
          <div className="space-y-3">
            <KeyValueList
              columns={2}
              items={[
                {
                  label: "Schema version",
                  value: manifestQuery.data.schema_version,
                },
                {
                  label: "Files",
                  value: manifestQuery.data.files.length,
                },
                {
                  label: "Created",
                  value: (
                    <RelativeTime
                      epochMs={manifestQuery.data.created_at_ms}
                    />
                  ),
                },
              ]}
            />
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-[11px]">
                <thead className="border-b border-border bg-surface text-left text-[10px] uppercase text-muted">
                  <tr>
                    <th className="px-2 py-1 font-semibold">Logical name</th>
                    <th className="px-2 py-1 font-semibold">Kind</th>
                    <th className="px-2 py-1 font-semibold">Path</th>
                    <th className="px-2 py-1 font-semibold">Bytes</th>
                    <th className="px-2 py-1 font-semibold">SHA-256</th>
                  </tr>
                </thead>
                <tbody>
                  {manifestQuery.data.files.map((f) => (
                    <tr
                      key={f.logical_name}
                      className="border-b border-border last:border-b-0"
                    >
                      <td className="px-2 py-1">{f.logical_name}</td>
                      <td className="px-2 py-1 text-muted">{f.kind}</td>
                      <td className="px-2 py-1 font-mono text-[10px]">
                        {f.relative_path}
                      </td>
                      <td className="px-2 py-1 tabular-nums">
                        {formatBytes(f.bytes)}
                      </td>
                      <td className="px-2 py-1 font-mono text-[10px] text-muted">
                        {f.sha256.slice(0, 12)}…
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <JsonViewer
              value={manifestQuery.data}
              label="Full manifest JSON"
              maxHeight={300}
            />
          </div>
        ) : null}
      </SectionCard>

      <SectionCard
        title="Checksums"
        toolbar={
          <Button size="sm" onClick={() => setShowChecksums((s) => !s)}>
            {showChecksums ? "Hide" : "Load checksums.sha256"}
          </Button>
        }
      >
        {!showChecksums ? (
          <StateEmpty message="Click 'Load checksums.sha256' to fetch the raw checksum file." />
        ) : checksumsQuery.isLoading ? (
          <Loading label="Loading checksums" />
        ) : checksumsQuery.isError ? (
          <Banner tone="danger" title="Could not load checksums">
            {(checksumsQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : checksumsQuery.data != null ? (
          <pre className="overflow-x-auto overflow-y-auto border border-border bg-surface p-2 font-mono text-[11px]" style={{ maxHeight: 300 }}>
            {checksumsQuery.data}
          </pre>
        ) : null}
      </SectionCard>

      <SectionCard title={`Linked transfers (${linkedTransfers.length})`}>
        {linkedTransfers.length === 0 ? (
          <StateEmpty message="No transfers reference this artifact." />
        ) : (
          <ul className="space-y-1">
            {linkedTransfers.slice(0, 20).map((t) => (
              <li key={t.id} className="text-[12px]">
                <Link
                  to={`/research/transfers/${encodeURIComponent(t.id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {t.id}
                </Link>
                <span className="ml-2 text-muted">
                  {t.source_machine_id ?? "—"} → {t.dest_machine_id ?? "—"}
                </span>
                <span className="ml-2 text-muted">{t.status}</span>
              </li>
            ))}
          </ul>
        )}
      </SectionCard>

      <SectionCard title={`Linked jobs (${linkedJobs.length})`}>
        {linkedJobs.length === 0 ? (
          <StateEmpty message="No jobs reference this artifact." />
        ) : (
          <ul className="space-y-1">
            {linkedJobs.slice(0, 20).map((j) => (
              <li key={j.id} className="text-[12px]">
                <Link
                  to={`/research/jobs/${encodeURIComponent(j.id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {j.id}
                </Link>
                <span className="ml-2 text-muted">{j.job_type}</span>
                <span className="ml-2 text-muted">{j.status}</span>
              </li>
            ))}
          </ul>
        )}
      </SectionCard>

      <SectionCard title={`Linked reports (${linkedReports.length})`}>
        {linkedReports.length === 0 ? (
          <StateEmpty message="No reports reference this artifact." />
        ) : (
          <ul className="space-y-1">
            {linkedReports.slice(0, 20).map((r) => (
              <li key={r.id} className="text-[12px]">
                <Link
                  to={`/research/reports/${encodeURIComponent(r.id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {r.title}
                </Link>
                <span className="ml-2 font-mono text-[11px] text-muted">
                  {r.id}
                </span>
              </li>
            ))}
          </ul>
        )}
      </SectionCard>

      <SectionCard title="Actions">
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={!role || !canPerform(role, "update")}
            title={role && canPerform(role, "update") ? undefined : permissionHint("update")}
            onClick={() => setEditOpen(true)}
          >
            Edit metadata
          </Button>
          {allowed.includes("verify") && (
            <ArtifactActionButton
              action="verify"
              role={role}
              pending={pendingAction === "verify"}
              onClick={() => verifyMutation.mutate()}
            />
          )}
          {allowed.includes("archive") && (
            <ArtifactActionButton
              action="archive"
              role={role}
              pending={pendingAction === "archive"}
              onClick={() => archiveMutation.mutate()}
            />
          )}
          {allowed.includes("restore") && (
            <ArtifactActionButton
              action="restore"
              role={role}
              pending={pendingAction === "restore"}
              onClick={() => restoreMutation.mutate()}
            />
          )}
          {allowed.includes("delete") && (
            <ArtifactActionButton
              action="delete"
              role={role}
              pending={false}
              onClick={() => setConfirmDeleteRecordOpen(true)}
            />
          )}
          {allowed.includes("delete_with_files") && (
            <ArtifactActionButton
              action="delete_with_files"
              role={role}
              pending={false}
              onClick={() => setConfirmDeleteFilesOpen(true)}
            />
          )}
          <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
            <InfoHint
              label="Labels and notes"
              text="Labels and notes require backend schema support that is not yet implemented."
            />
            Labels/notes: unsupported
          </span>
        </div>
      </SectionCard>

      <EditArtifactDialog
        open={editOpen}
        onClose={() => setEditOpen(false)}
        artifactId={artifact.id}
        initial={{
          source_machine_id: artifact.source_machine_id ?? "",
          run_mode: artifact.run_mode ?? "",
          replay_quality_class: artifact.replay_quality_class ?? "",
          backtest_ready_class: artifact.backtest_ready_class ?? "",
          live_fidelity_class: artifact.live_fidelity_class ?? "",
        }}
        onSaved={invalidate}
      />

      <ConfirmDialog
        open={confirmDeleteRecordOpen}
        title="Delete artifact record"
        description="Removes the artifact metadata only. Files on disk are not touched."
        confirmLabel="Delete record"
        destructive
        pending={deleteRecordMutation.isPending}
        errorMessage={
          deleteRecordMutation.isError
            ? (deleteRecordMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteRecordMutation.mutate()}
        onClose={() => setConfirmDeleteRecordOpen(false)}
      />

      <ConfirmDialog
        open={confirmDeleteFilesOpen}
        title="Delete artifact and files"
        description="Type the artifact ID to confirm. Removes the metadata record AND the files on disk under the artifact root."
        confirmLabel="Delete artifact and files"
        phrase={artifact.id}
        destructive
        pending={deleteFilesMutation.isPending}
        errorMessage={
          deleteFilesMutation.isError
            ? (deleteFilesMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteFilesMutation.mutate()}
        onClose={() => setConfirmDeleteFilesOpen(false)}
      />
    </div>
  );
}

interface ArtifactActionButtonProps {
  action: ResearchAction;
  role: "admin" | "observer" | undefined;
  pending: boolean;
  onClick: () => void;
}

function ArtifactActionButton({
  action,
  role,
  pending,
  onClick,
}: ArtifactActionButtonProps) {
  const gate = getActionGateState(role, action);
  const disabled = gate !== "enabled";
  const tone =
    action === "delete" || action === "delete_with_files" ? "danger" : "neutral";
  return (
    <Button
      size="sm"
      tone={tone}
      disabled={disabled || pending}
      state={pending ? "pending" : "idle"}
      title={disabled ? permissionHint(action) : undefined}
      onClick={onClick}
    >
      {ACTION_LABELS[action]}
    </Button>
  );
}

interface EditArtifactDialogProps {
  open: boolean;
  onClose: () => void;
  artifactId: string;
  initial: {
    source_machine_id: string;
    run_mode: string;
    replay_quality_class: string;
    backtest_ready_class: string;
    live_fidelity_class: string;
  };
  onSaved: () => void;
}

function EditArtifactDialog({
  open,
  onClose,
  artifactId,
  initial,
  onSaved,
}: EditArtifactDialogProps) {
  const queryClient = useQueryClient();
  const [sourceMachineId, setSourceMachineId] = useState(initial.source_machine_id);
  const [runMode, setRunMode] = useState(initial.run_mode);
  const [replayQuality, setReplayQuality] = useState(initial.replay_quality_class);
  const [backtestReady, setBacktestReady] = useState(initial.backtest_ready_class);
  const [liveFidelity, setLiveFidelity] = useState(initial.live_fidelity_class);
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (req: UpdateArtifactRequest) =>
      updateResearchArtifact(artifactId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["research", "artifact", artifactId],
      });
      queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
      onSaved();
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const submit = () => {
    const req: UpdateArtifactRequest = {};
    if (sourceMachineId.trim()) req.source_machine_id = sourceMachineId.trim();
    if (runMode.trim()) req.run_mode = runMode.trim();
    if (replayQuality.trim()) req.replay_quality_class = replayQuality.trim();
    if (backtestReady.trim()) req.backtest_ready_class = backtestReady.trim();
    if (liveFidelity.trim()) req.live_fidelity_class = liveFidelity.trim();
    mutation.mutate(req);
  };

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Edit artifact metadata"
    >
      <div className="space-y-3">
        <FormField label="Source machine ID">
          {({ id }) => (
            <Input
              id={id}
              value={sourceMachineId}
              onChange={(e) => setSourceMachineId(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Run mode">
          {({ id }) => (
            <Input
              id={id}
              value={runMode}
              onChange={(e) => setRunMode(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Replay quality class">
          {({ id }) => (
            <Input
              id={id}
              value={replayQuality}
              onChange={(e) => setReplayQuality(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Backtest readiness class">
          {({ id }) => (
            <Input
              id={id}
              value={backtestReady}
              onChange={(e) => setBacktestReady(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Live fidelity class">
          {({ id }) => (
            <Input
              id={id}
              value={liveFidelity}
              onChange={(e) => setLiveFidelity(e.currentTarget.value)}
            />
          )}
        </FormField>
        {error && (
          <Banner tone="danger" title="Could not update">
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
            disabled={mutation.isPending}
            state={mutation.isPending ? "pending" : "idle"}
          >
            Save
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
