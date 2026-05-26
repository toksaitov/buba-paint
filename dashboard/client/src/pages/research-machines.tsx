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
  Segment,
  StateEmpty,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { Dialog } from "../components/ui/dialog";
import { StatusFilter } from "../components/research/status-filter";
import { useResearchMachines } from "../hooks/use-research-machines";
import { useAuthStore } from "../stores/auth-store";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createResearchMachine } from "../lib/research-api";
import { machineTone } from "../lib/research-permissions";
import { humanize } from "../lib/utils";
import type {
  CreateMachineRequest,
  MachineRole,
  MachineStatus,
} from "../lib/research-types";

const ALL_STATUSES: MachineStatus[] = [
  "not_configured",
  "configured",
  "online",
  "idle",
  "busy",
  "degraded",
  "error",
  "disabled",
  "unreachable",
  "maintenance",
];

const ROLES: MachineRole[] = ["live", "research", "controller"];

export function ResearchMachinesPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const machinesQuery = useResearchMachines();
  const [active, setActive] = useState<string[]>([...ALL_STATUSES]);
  const [createOpen, setCreateOpen] = useState(false);

  const machinesData = machinesQuery.data?.machines;
  const filtered = useMemo(
    () => (machinesData ?? []).filter((m) => active.includes(m.status)),
    [machinesData, active],
  );

  if (machinesQuery.isLoading) {
    return <Loading label="Loading machines" />;
  }
  if (machinesQuery.isError) {
    return (
      <Banner tone="danger" title="Could not load machines">
        {(machinesQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  return (
    <div className="space-y-3">
      <SectionCard
        title="All machines"
        toolbar={
          <Button
            size="sm"
            tone="accent"
            iconLeft={<Plus size={14} />}
            onClick={() => setCreateOpen(true)}
            disabled={!isAdmin}
            title={isAdmin ? undefined : "Admin role required."}
          >
            New machine
          </Button>
        }
      >
        <StatusFilter
          label="Status"
          statuses={ALL_STATUSES}
          active={active}
          onChange={setActive}
          toneFor={(s) => machineTone(s as MachineStatus)}
          ariaLabel="Machine status filter"
        />
        {filtered.length === 0 ? (
          <StateEmpty message="No machines match the selected filters." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-[12px]">
              <thead className="border-b border-border bg-surface text-left text-[11px] uppercase text-muted">
                <tr>
                  <th className="px-2 py-1.5 font-semibold">ID</th>
                  <th className="px-2 py-1.5 font-semibold">Name</th>
                  <th className="px-2 py-1.5 font-semibold">Role</th>
                  <th className="px-2 py-1.5 font-semibold">Status</th>
                  <th className="px-2 py-1.5 font-semibold">SSH alias</th>
                  <th className="px-2 py-1.5 font-semibold">Updated</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((machine) => (
                  <tr
                    key={machine.id}
                    className="border-b border-border last:border-b-0 hover:bg-surface"
                  >
                    <td className="px-2 py-1.5">
                      <Link
                        to={`/research/machines/${encodeURIComponent(machine.id)}`}
                        className="font-mono text-[11px] hover:underline"
                      >
                        {machine.id}
                      </Link>
                    </td>
                    <td className="px-2 py-1.5">{machine.name}</td>
                    <td className="px-2 py-1.5 text-muted">
                      {humanize(machine.role)}
                    </td>
                    <td className="px-2 py-1.5">
                      <StatusChip
                        label={humanize(machine.status)}
                        tone={machineTone(machine.status)}
                        compact
                      />
                    </td>
                    <td className="px-2 py-1.5 font-mono text-[11px] text-muted">
                      {machine.ssh_alias ?? "—"}
                    </td>
                    <td className="px-2 py-1.5 text-muted">
                      <RelativeTime epochMs={machine.updated_at} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>
      <CreateMachineDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
      />
    </div>
  );
}

interface CreateMachineDialogProps {
  open: boolean;
  onClose: () => void;
}

function CreateMachineDialog({ open, onClose }: CreateMachineDialogProps) {
  const queryClient = useQueryClient();
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [role, setRole] = useState<MachineRole>("research");
  const [sshAlias, setSshAlias] = useState("");
  const [details, setDetails] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (req: CreateMachineRequest) => createResearchMachine(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["research", "machines"] });
      setId("");
      setName("");
      setSshAlias("");
      setDetails("");
      setError(null);
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const submit = () => {
    setError(null);
    let detailsObj: Record<string, unknown> | undefined;
    if (details.trim()) {
      try {
        detailsObj = JSON.parse(details);
      } catch (err) {
        setError(`Details JSON is invalid: ${(err as Error).message}`);
        return;
      }
    }
    mutation.mutate({
      id: id.trim(),
      name: name.trim(),
      role,
      ssh_alias: sshAlias.trim() || undefined,
      details: detailsObj,
    });
  };

  return (
    <Dialog
      open={open}
      onClose={mutation.isPending ? () => undefined : onClose}
      title="Register machine"
      description="Add a custom research host alongside the seeded live and research defaults."
    >
      <div className="space-y-3">
        <FormField label="ID" required hint="Stable identifier, e.g. lab-01">
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={id}
              onChange={(e) => setId(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Display name" required>
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={name}
              onChange={(e) => setName(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Role" required>
          {() => (
            <Segment
              value={role}
              onChange={(v) => setRole(v as MachineRole)}
              items={ROLES.map((r) => ({ value: r, label: r }))}
              ariaLabel="Machine role"
            />
          )}
        </FormField>
        <FormField label="SSH alias" hint="Optional">
          {({ id: fieldId }) => (
            <Input
              id={fieldId}
              value={sshAlias}
              onChange={(e) => setSshAlias(e.currentTarget.value)}
            />
          )}
        </FormField>
        <FormField label="Details JSON" hint="Optional metadata object">
          {({ id: fieldId }) => (
            <Textarea
              id={fieldId}
              value={details}
              onChange={(e) => setDetails(e.currentTarget.value)}
              minRows={3}
              placeholder='{"host": "..."}'
            />
          )}
        </FormField>
        {error && (
          <Banner tone="danger" title="Could not register">
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
            disabled={!id.trim() || !name.trim() || mutation.isPending}
            state={mutation.isPending ? "pending" : "idle"}
          >
            Register
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
