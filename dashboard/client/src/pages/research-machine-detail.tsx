import { useMemo, useState } from "react";
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
  Segment,
  StateEmpty,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { Dialog } from "../components/ui/dialog";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { JsonViewer } from "../components/research/json-viewer";
import {
  useResearchMachine,
  useResearchMachineHealth,
  useResearchMachines,
} from "../hooks/use-research-machines";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import { useResearchTransfers } from "../hooks/use-research-transfers";
import { useAuthStore } from "../stores/auth-store";
import {
  deleteResearchMachine,
  disableResearchMachine,
  enableResearchMachine,
  updateResearchMachine,
} from "../lib/research-api";
import {
  ACTION_LABELS,
  canPerform,
  getActionGateState,
  getAllowedActions,
  machineTone,
  permissionHint,
} from "../lib/research-permissions";
import type {
  MachineRole,
  ResearchAction,
  UpdateMachineRequest,
} from "../lib/research-types";
import { humanize } from "../lib/utils";

const ROLES: MachineRole[] = ["live", "research", "controller"];
const DEFAULT_MACHINE_IDS = new Set(["live", "research"]);

export function ResearchMachineDetailPage() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const role = user?.role;

  const machineQuery = useResearchMachine(id);
  const healthQuery = useResearchMachineHealth(id);
  const artifactsQuery = useResearchArtifacts();
  const transfersQuery = useResearchTransfers();
  const machinesQuery = useResearchMachines();

  const [editOpen, setEditOpen] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<ResearchAction | null>(
    null,
  );

  const invalidateMachine = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "machines"] });
    queryClient.invalidateQueries({
      queryKey: ["research", "machine", id],
    });
  };

  const disableMutation = useMutation({
    mutationFn: () => disableResearchMachine(id),
    onMutate: () => {
      setActionError(null);
      setPendingAction("disable");
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidateMachine();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const enableMutation = useMutation({
    mutationFn: () => enableResearchMachine(id),
    onMutate: () => {
      setActionError(null);
      setPendingAction("enable");
    },
    onSuccess: () => {
      setPendingAction(null);
      invalidateMachine();
    },
    onError: (err: Error) => {
      setPendingAction(null);
      setActionError(err.message);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteResearchMachine(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "machines"] });
      navigate("/research/machines");
    },
    onError: (err: Error) => setActionError(err.message),
  });

  if (machineQuery.isLoading) {
    return <Loading label="Loading machine" />;
  }
  if (machineQuery.isError || !machineQuery.data) {
    return (
      <Banner tone="danger" title="Could not load machine">
        {(machineQuery.error as Error)?.message ?? "Machine not found."}
      </Banner>
    );
  }

  const machine = machineQuery.data.machine;
  const health = healthQuery.data;
  const deps = health?.dependencies;
  const isDefault = DEFAULT_MACHINE_IDS.has(machine.id);
  const hasDependencies = deps != null && depHasAny(deps);

  const linkedArtifacts = (artifactsQuery.data?.artifacts ?? []).filter(
    (a) => a.source_machine_id === machine.id,
  );
  const linkedTransfers = (transfersQuery.data?.transfers ?? []).filter(
    (t) =>
      t.source_machine_id === machine.id || t.dest_machine_id === machine.id,
  );

  const allowed = getAllowedActions("machine", machine.status, {
    is_default_machine: isDefault,
    has_dependencies: hasDependencies,
  });

  const identityItems = [
    { label: "ID", value: <span className="font-mono">{machine.id}</span> },
    { label: "Name", value: machine.name },
    { label: "Role", value: humanize(machine.role) },
    {
      label: "SSH alias",
      value:
        machine.ssh_alias == null ? (
          <span className="text-muted">—</span>
        ) : (
          <span className="font-mono text-[11px]">{machine.ssh_alias}</span>
        ),
    },
    { label: "Status", value: humanize(machine.status) },
    {
      label: "Created",
      value: <RelativeTime epochMs={machine.created_at} />,
    },
    {
      label: "Updated",
      value: <RelativeTime epochMs={machine.updated_at} />,
    },
  ];

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Link
          to="/research/machines"
          className="text-[12px] text-muted hover:underline"
        >
          ← Machines
        </Link>
        <span className="text-[14px] font-semibold">{machine.name}</span>
        <span className="font-mono text-[11px] text-muted">{machine.id}</span>
        <StatusChip
          label={humanize(machine.status)}
          tone={machineTone(machine.status)}
        />
      </div>
      {actionError && (
        <Banner tone="danger" title="Action failed">
          {actionError}
        </Banner>
      )}
      {isDefault && (
        <Banner tone="info" title="Default machine">
          The seeded `live` and `research` machines anchor the pipeline. They
          can be disabled but not deleted.
        </Banner>
      )}
      {health?.disabled && (
        <Banner tone="warning" title="Machine disabled">
          The worker will skip leases for this host until it is re-enabled.
        </Banner>
      )}
      <SectionCard
        title="Identity"
        toolbar={
          <Button
            size="sm"
            disabled={!role || !canPerform(role, "update")}
            title={
              role && canPerform(role, "update")
                ? undefined
                : permissionHint("update")
            }
            onClick={() => setEditOpen(true)}
          >
            Edit
          </Button>
        }
      >
        <KeyValueList items={identityItems} columns={2} />
      </SectionCard>

      <SectionCard title="Health">
        {healthQuery.isLoading ? (
          <Loading label="Loading health" />
        ) : healthQuery.isError ? (
          <Banner tone="danger" title="Could not load health">
            {(healthQuery.error as Error)?.message ?? "Unknown error"}
          </Banner>
        ) : health == null ? (
          <StateEmpty message="No health snapshot yet." />
        ) : (
          <div className="space-y-3">
            <KeyValueList
              items={[
                { label: "Artifacts", value: depString(deps?.artifacts) },
                {
                  label: "Transfers (source)",
                  value: depString(deps?.transfers_as_source),
                },
                {
                  label: "Transfers (destination)",
                  value: depString(deps?.transfers_as_destination),
                },
                {
                  label: "Active transfers",
                  value: depString(deps?.active_transfers),
                },
                {
                  label: "Jobs using source artifacts",
                  value: depString(deps?.jobs_using_source_artifacts),
                },
                {
                  label: "Reports using source artifacts",
                  value: depString(deps?.reports_using_source_artifacts),
                },
              ]}
              columns={2}
            />
            <JsonViewer
              value={health.details ?? null}
              label="Worker telemetry"
              emptyLabel="No telemetry recorded. Heartbeats are issued by the research-worker process, not the dashboard."
              maxHeight={240}
            />
          </div>
        )}
      </SectionCard>

      <SectionCard
        title={`Linked artifacts (${linkedArtifacts.length})`}
      >
        {linkedArtifacts.length === 0 ? (
          <StateEmpty message="No artifacts source from this machine." />
        ) : (
          <ul className="space-y-1">
            {linkedArtifacts.slice(0, 20).map((a) => (
              <li key={a.id} className="text-[12px]">
                <Link
                  to={`/research/artifacts/${encodeURIComponent(a.id)}`}
                  className="font-mono text-[11px] hover:underline"
                >
                  {a.id}
                </Link>
                <span className="ml-2 text-muted">{a.kind}</span>
                <span className="ml-2 text-muted">{a.status}</span>
              </li>
            ))}
          </ul>
        )}
      </SectionCard>

      <SectionCard
        title={`Linked transfers (${linkedTransfers.length})`}
      >
        {linkedTransfers.length === 0 ? (
          <StateEmpty message="No transfers reference this machine." />
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

      <SectionCard title="Actions">
        <div className="flex flex-wrap items-center gap-2">
          {allowed
            .filter((action) => action !== "update")
            .map((action) => (
              <MachineActionButton
                key={action}
                action={action}
                role={role}
                pending={pendingAction === action}
                onClick={() => {
                  if (action === "disable") return disableMutation.mutate();
                  if (action === "enable") return enableMutation.mutate();
                  if (action === "delete") return setConfirmDeleteOpen(true);
                  if (action === "health")
                    return healthQuery.refetch().then(() => undefined);
                }}
              />
            ))}
          {!allowed.includes("delete") && (
            <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
              <InfoHint
                label="Delete"
                text={
                  isDefault
                    ? "Default machines cannot be deleted. Disable instead."
                    : hasDependencies
                      ? "Cannot delete: machine has artifacts, transfers, jobs, or reports referencing it."
                      : "Delete unavailable."
                }
              />
              Delete unavailable
            </span>
          )}
        </div>
      </SectionCard>

      <Banner tone="info" title="Worker process model">
        Heartbeats, lease acquisition, and step execution happen in the
        research-worker process on the host. The dashboard observes and steers;
        it does not run jobs.
      </Banner>

      <EditMachineDialog
        open={editOpen}
        onClose={() => setEditOpen(false)}
        machineId={machine.id}
        initial={{
          name: machine.name,
          role: machine.role,
          ssh_alias: machine.ssh_alias ?? "",
          details: machine.details_json ?? "",
        }}
        onSaved={invalidateMachine}
      />

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="Delete machine"
        description="Type the machine ID to confirm. This removes the record only; deployed worker processes are not stopped."
        phrase={machine.id}
        destructive
        confirmLabel="Delete machine"
        pending={deleteMutation.isPending}
        errorMessage={
          deleteMutation.isError
            ? (deleteMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => deleteMutation.mutate()}
        onClose={() => setConfirmDeleteOpen(false)}
      />

      <div className="text-[11px] text-muted">
        {machinesQuery.data?.machines.length ?? 0} machines registered
      </div>
    </div>
  );
}

function depString(value: number | undefined): string {
  return value == null ? "—" : String(value);
}

function depHasAny(d: {
  artifacts: number;
  transfers_as_source: number;
  transfers_as_destination: number;
  active_transfers: number;
  jobs_using_source_artifacts: number;
  reports_using_source_artifacts: number;
}): boolean {
  return (
    d.artifacts +
      d.transfers_as_source +
      d.transfers_as_destination +
      d.active_transfers +
      d.jobs_using_source_artifacts +
      d.reports_using_source_artifacts >
    0
  );
}

interface MachineActionButtonProps {
  action: ResearchAction;
  role: "admin" | "observer" | undefined;
  pending: boolean;
  onClick: () => void;
}

function MachineActionButton({
  action,
  role,
  pending,
  onClick,
}: MachineActionButtonProps) {
  const gate = getActionGateState(role, action);
  const disabled = gate !== "enabled";
  return (
    <Button
      size="sm"
      tone={action === "delete" ? "danger" : "neutral"}
      disabled={disabled || pending}
      state={pending ? "pending" : "idle"}
      title={disabled ? permissionHint(action) : undefined}
      onClick={onClick}
    >
      {ACTION_LABELS[action]}
    </Button>
  );
}

interface EditMachineDialogProps {
  open: boolean;
  onClose: () => void;
  machineId: string;
  initial: {
    name: string;
    role: MachineRole;
    ssh_alias: string;
    details: string;
  };
  onSaved: () => void;
}

function EditMachineDialog({
  open,
  onClose,
  machineId,
  initial,
  onSaved,
}: EditMachineDialogProps) {
  const [name, setName] = useState(initial.name);
  const [role, setRole] = useState<MachineRole>(initial.role);
  const [sshAlias, setSshAlias] = useState(initial.ssh_alias);
  const [details, setDetails] = useState(initial.details);
  const [error, setError] = useState<string | null>(null);

  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: (req: UpdateMachineRequest) =>
      updateResearchMachine(machineId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "machine", machineId] });
      queryClient.invalidateQueries({ queryKey: ["research", "machines"] });
      onSaved();
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const submit = () => {
    setError(null);
    let detailsObj: Record<string, unknown> | null = null;
    if (details.trim()) {
      try {
        detailsObj = JSON.parse(details) as Record<string, unknown>;
      } catch (err) {
        setError(`Details JSON is invalid: ${(err as Error).message}`);
        return;
      }
    }
    mutation.mutate({
      name: name.trim() || undefined,
      role,
      ssh_alias: sshAlias.trim() ? sshAlias.trim() : null,
      details: detailsObj,
    });
  };

  const initialItems = useMemo(
    () => ROLES.map((r) => ({ value: r, label: r })),
    [],
  );

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Edit machine"
    >
      <div className="space-y-3">
        <FormField label="Display name">
          {({ id }) => (
            <Input
              id={id}
              value={name}
              onChange={(e) => setName(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Role">
          {() => (
            <Segment
              value={role}
              onChange={(v) => setRole(v as MachineRole)}
              items={initialItems}
              ariaLabel="Machine role"
            />
          )}
        </FormField>
        <FormField label="SSH alias" hint="Empty value clears the alias.">
          {({ id }) => (
            <Input
              id={id}
              value={sshAlias}
              onChange={(e) => setSshAlias(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Details JSON" hint="Empty value clears the field.">
          {({ id }) => (
            <Textarea
              id={id}
              value={details}
              onChange={(e) => setDetails(e.currentTarget.value)}
              minRows={4}
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
