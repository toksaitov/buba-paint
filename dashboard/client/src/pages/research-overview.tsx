import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Banner,
  Button,
  FormField,
  Input,
  MetricCard,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { ConfirmDialog } from "../components/research/confirm-dialog";
import { Dialog } from "../components/ui/dialog";
import { useResearchArtifacts } from "../hooks/use-research-artifacts";
import {
  useResearchJobTemplates,
  useResearchQueue,
  useResearchRetention,
} from "../hooks/use-research-templates";
import { useAuthStore } from "../stores/auth-store";
import {
  archiveResearchJobTemplate,
  archiveResearchRetention,
  createResearchJobTemplate,
  deleteResearchJobTemplate,
  restoreResearchJobTemplate,
  updateResearchJobTemplate,
} from "../lib/research-api";
import {
  jobTone,
  jobTypeLabel,
  machineTone,
  reportTone,
  transferTone,
} from "../lib/research-permissions";
import type {
  ResearchJobTemplate,
  ResearchQueueJobItem,
  ResearchQueueResponse,
  ResearchQueueTransferItem,
  ResearchRetentionArtifactCandidate,
  ResearchRetentionJobCandidate,
  ResearchRetentionReportCandidate,
  RetentionArchiveResponse,
  UpsertJobTemplateRequest,
} from "../lib/research-types";
import { formatBytes, humanize } from "../lib/utils";

type TemplateDialogMode = "create" | "edit";

interface TemplateDialogState {
  mode: TemplateDialogMode;
  template: ResearchJobTemplate | null;
}

interface RetentionSelection {
  jobIds: string[];
  reportIds: string[];
  artifactIds: string[];
}

const EMPTY_SELECTION: RetentionSelection = {
  jobIds: [],
  reportIds: [],
  artifactIds: [],
};

export function ResearchOverviewPage() {
  const queryClient = useQueryClient();
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === "admin";
  const queueQuery = useResearchQueue();
  const retentionQuery = useResearchRetention();
  const templatesQuery = useResearchJobTemplates();
  const artifactsQuery = useResearchArtifacts();
  const [templateDialog, setTemplateDialog] =
    useState<TemplateDialogState | null>(null);
  const [confirmTemplateDelete, setConfirmTemplateDelete] =
    useState<ResearchJobTemplate | null>(null);
  const [retentionSelection, setRetentionSelection] =
    useState<RetentionSelection>(EMPTY_SELECTION);
  const [confirmRetentionOpen, setConfirmRetentionOpen] = useState(false);
  const [archiveResult, setArchiveResult] =
    useState<RetentionArchiveResponse | null>(null);

  const templates = templatesQuery.data?.templates ?? [];
  const retention = retentionQuery.data;
  const selectedCounts =
    retentionSelection.jobIds.length +
    retentionSelection.reportIds.length +
    retentionSelection.artifactIds.length;

  const invalidateResearch = () => {
    queryClient.invalidateQueries({ queryKey: ["research", "queue"] });
    queryClient.invalidateQueries({ queryKey: ["research", "retention"] });
    queryClient.invalidateQueries({ queryKey: ["research", "job-templates"] });
    queryClient.invalidateQueries({ queryKey: ["research", "jobs"] });
    queryClient.invalidateQueries({ queryKey: ["research", "reports"] });
    queryClient.invalidateQueries({ queryKey: ["research", "artifacts"] });
  };

  const templateMutation = useMutation({
    mutationFn: (req: UpsertJobTemplateRequest) =>
      templateDialog?.mode === "edit" && templateDialog.template
        ? updateResearchJobTemplate(templateDialog.template.id, req)
        : createResearchJobTemplate(req),
    onSuccess: () => {
      setTemplateDialog(null);
      invalidateResearch();
    },
  });

  const templateActionMutation = useMutation({
    mutationFn: async ({
      action,
      id,
    }: {
      action: "archive" | "restore" | "delete";
      id: string;
    }) => {
      if (action === "archive") return archiveResearchJobTemplate(id);
      if (action === "restore") return restoreResearchJobTemplate(id);
      return deleteResearchJobTemplate(id);
    },
    onSuccess: invalidateResearch,
  });

  const retentionMutation = useMutation({
    mutationFn: () =>
      archiveResearchRetention({
        job_ids: retentionSelection.jobIds,
        report_ids: retentionSelection.reportIds,
        artifact_ids: retentionSelection.artifactIds,
      }),
    onSuccess: (result) => {
      setArchiveResult(result);
      setConfirmRetentionOpen(false);
      setRetentionSelection(EMPTY_SELECTION);
      invalidateResearch();
    },
  });

  const scratchBytes = useMemo(() => {
    if (!retention) return 0;
    return retention.jobs
      .filter((candidate) =>
        retentionSelection.jobIds.includes(candidate.job.id),
      )
      .reduce((total, candidate) => total + candidate.scratch_bytes, 0);
  }, [retention, retentionSelection.jobIds]);

  if (queueQuery.isLoading) {
    return <Loading label="Loading research queue" />;
  }
  if (queueQuery.isError || !queueQuery.data) {
    return (
      <Banner tone="danger" title="Could not load research queue">
        {(queueQuery.error as Error)?.message ?? "Unknown error"}
      </Banner>
    );
  }

  const queue = queueQuery.data;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <MetricCard
          label="Active jobs"
          value={queue.counts.jobs_active.toString()}
          sub={`${queue.counts.jobs_waiting} waiting`}
        />
        <MetricCard
          label="Attention"
          value={(
            queue.counts.jobs_blocked +
            queue.counts.jobs_failed +
            queue.counts.jobs_retryable +
            queue.counts.stale_leases +
            queue.counts.transfers_attention
          ).toString()}
          tone={
            queue.counts.jobs_blocked +
              queue.counts.jobs_failed +
              queue.counts.transfers_attention >
            0
              ? "danger"
              : "neutral"
          }
          sub={`${queue.counts.stale_leases} stale leases`}
        />
        <MetricCard
          label="Transfers"
          value={queue.counts.transfers_active.toString()}
          sub={`${queue.counts.transfers_attention} attention`}
        />
        <MetricCard
          label="Retention candidates"
          value={(
            queue.retention.jobs +
            queue.retention.reports +
            queue.retention.artifacts
          ).toString()}
          sub={`${formatBytes(
            queue.retention.scratch_bytes +
              queue.retention.report_bytes +
              queue.retention.artifact_bytes,
          )} reclaimable`}
        />
      </div>

      <div className="grid gap-3 xl:grid-cols-[1.3fr_1fr]">
        <SectionCard title="Queue cockpit">
          {queueIsIdle(queue) ? (
            <p className="text-[12px] text-muted">
              The queue is idle. No jobs are running, waiting, or needing
              attention.
            </p>
          ) : (
            <div className="grid gap-3 lg:grid-cols-2">
              <QueueJobGroup title="Running" items={queue.jobs.running} />
              <QueueJobGroup title="Waiting" items={queue.jobs.waiting} />
              <QueueJobGroup title="Retryable" items={queue.jobs.retryable} />
              <QueueJobGroup title="Blocked" items={queue.jobs.blocked} />
              <QueueJobGroup title="Failed" items={queue.jobs.failed} />
              <QueueJobGroup
                title="Stale leases"
                items={queue.jobs.stale_leases}
              />
            </div>
          )}
        </SectionCard>

        <SectionCard title="Transfer attention">
          {queue.transfers.active.length === 0 &&
          queue.transfers.attention.length === 0 &&
          queue.transfers.stale.length === 0 ? (
            <p className="text-[12px] text-muted">
              No transfers are active or needing attention.
            </p>
          ) : (
            <div className="space-y-3">
              <QueueTransferGroup
                title="Active"
                items={queue.transfers.active}
              />
              <QueueTransferGroup
                title="Attention"
                items={queue.transfers.attention}
              />
              <QueueTransferGroup title="Stale" items={queue.transfers.stale} />
            </div>
          )}
        </SectionCard>
      </div>

      {queue.disabled_hosts.length > 0 && (
        <SectionCard title="Disabled research hosts">
          <div className="space-y-2">
            {queue.disabled_hosts.map(({ machine, dependencies }) => (
              <Link
                key={machine.id}
                to={`/research/machines/${encodeURIComponent(machine.id)}`}
                className="flex flex-wrap items-center justify-between gap-2 border border-border px-2 py-1.5 hover:bg-surface"
              >
                <div className="flex items-center gap-2">
                  <StatusChip
                    label={humanize(machine.status)}
                    tone={machineTone(machine.status)}
                    compact
                  />
                  <span className="font-semibold">{machine.name}</span>
                  <span className="font-mono text-[11px] text-muted">
                    {machine.id}
                  </span>
                </div>
                <span className="text-[11px] text-muted">
                  {dependencies.active_transfers} active transfers
                </span>
              </Link>
            ))}
          </div>
        </SectionCard>
      )}

      <div className="grid gap-3 xl:grid-cols-[1fr_1fr]">
        <SectionCard
          title="Templates"
          toolbar={
            <Button
              size="sm"
              tone="accent"
              disabled={!isAdmin}
              title={isAdmin ? undefined : "Admin role required."}
              onClick={() =>
                setTemplateDialog({ mode: "create", template: null })
              }
            >
              New template
            </Button>
          }
        >
          {templatesQuery.isLoading ? (
            <Loading label="Loading templates" />
          ) : templatesQuery.isError ? (
            <Banner tone="danger" title="Could not load templates">
              {(templatesQuery.error as Error)?.message ?? "Unknown error"}
            </Banner>
          ) : templates.length === 0 ? (
            <StateEmpty message="No job templates yet." />
          ) : (
            <div className="space-y-2">
              {templates.map((template) => (
                <TemplateRow
                  key={template.id}
                  template={template}
                  isAdmin={isAdmin}
                  pending={templateActionMutation.isPending}
                  onEdit={() =>
                    setTemplateDialog({ mode: "edit", template })
                  }
                  onAction={(action) =>
                    action === "delete"
                      ? setConfirmTemplateDelete(template)
                      : templateActionMutation.mutate({
                          action,
                          id: template.id,
                        })
                  }
                />
              ))}
            </div>
          )}
          {templateActionMutation.isError && (
            <Banner tone="danger" title="Template action failed">
              {(templateActionMutation.error as Error)?.message ??
                "Unknown error"}
            </Banner>
          )}
        </SectionCard>

        <SectionCard
          title="Retention"
          toolbar={
            <Button
              size="sm"
              tone="accent"
              disabled={!isAdmin || selectedCounts === 0}
              title={
                isAdmin
                  ? selectedCounts === 0
                    ? "Select retention candidates first."
                    : undefined
                  : "Admin role required."
              }
              onClick={() => setConfirmRetentionOpen(true)}
            >
              Archive selected
            </Button>
          }
        >
          {retentionQuery.isLoading ? (
            <Loading label="Loading retention" />
          ) : retentionQuery.isError || !retention ? (
            <Banner tone="danger" title="Could not load retention">
              {(retentionQuery.error as Error)?.message ?? "Unknown error"}
            </Banner>
          ) : (
            <RetentionPanel
              jobs={retention.jobs}
              reports={retention.reports}
              artifacts={retention.artifacts}
              selection={retentionSelection}
              setSelection={setRetentionSelection}
            />
          )}
          {archiveResult && <RetentionResult result={archiveResult} />}
        </SectionCard>
      </div>

      <SectionCard title="Recent reports">
        {queue.recent_reports.length === 0 ? (
          <StateEmpty message="No reports yet." />
        ) : (
          <div className="space-y-2">
            {queue.recent_reports.map((report) => (
              <Link
                key={report.id}
                to={`/research/reports/${encodeURIComponent(report.id)}`}
                className="flex flex-wrap items-center justify-between gap-2 border border-border px-2 py-1.5 hover:bg-surface"
              >
                <div className="min-w-0">
                  <div className="truncate font-semibold">{report.title}</div>
                  <div className="font-mono text-[11px] text-muted">
                    {report.id}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <StatusChip
                    label={humanize(report.status)}
                    tone={reportTone(report.status)}
                    compact
                  />
                  <RelativeTime epochMs={report.updated_at} />
                </div>
              </Link>
            ))}
          </div>
        )}
      </SectionCard>

      <TemplateDialog
        key={templateDialog?.template?.id ?? templateDialog?.mode ?? "closed"}
        open={templateDialog != null}
        mode={templateDialog?.mode ?? "create"}
        template={templateDialog?.template ?? null}
        artifacts={artifactsQuery.data?.artifacts ?? []}
        pending={templateMutation.isPending}
        error={
          templateMutation.isError
            ? ((templateMutation.error as Error)?.message ??
              "Template save failed")
            : null
        }
        onSubmit={(req) => templateMutation.mutate(req)}
        onClose={() => {
          setTemplateDialog(null);
          templateMutation.reset();
        }}
      />

      <ConfirmDialog
        open={confirmTemplateDelete != null}
        title="Delete job template"
        description={`Deletes template "${confirmTemplateDelete?.name ?? ""}" from the shared template list. Existing jobs and reports remain unchanged.`}
        confirmLabel="Delete template"
        pending={templateActionMutation.isPending}
        errorMessage={
          templateActionMutation.isError
            ? (templateActionMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => {
          if (!confirmTemplateDelete) return;
          templateActionMutation.mutate(
            { action: "delete", id: confirmTemplateDelete.id },
            { onSuccess: () => setConfirmTemplateDelete(null) },
          );
        }}
        onClose={() => {
          if (!templateActionMutation.isPending) setConfirmTemplateDelete(null);
        }}
      />

      <ConfirmDialog
        open={confirmRetentionOpen}
        title="Archive selected retention candidates"
        description={`Archives metadata and scratch DBs for ${selectedCounts} selected candidate${selectedCounts === 1 ? "" : "s"} and estimates ${formatBytes(scratchBytes)} of scratch DB cleanup. Report and artifact files are preserved.`}
        confirmLabel="Archive selected"
        pending={retentionMutation.isPending}
        errorMessage={
          retentionMutation.isError
            ? (retentionMutation.error as Error)?.message
            : undefined
        }
        onConfirm={() => retentionMutation.mutate()}
        onClose={() => setConfirmRetentionOpen(false)}
      />
    </div>
  );
}

function queueIsIdle(queue: ResearchQueueResponse): boolean {
  return (
    queue.jobs.running.length === 0 &&
    queue.jobs.waiting.length === 0 &&
    queue.jobs.retryable.length === 0 &&
    queue.jobs.blocked.length === 0 &&
    queue.jobs.failed.length === 0 &&
    queue.jobs.stale_leases.length === 0
  );
}

function QueueJobGroup({
  title,
  items,
}: {
  title: string;
  items: ResearchQueueJobItem[];
}) {
  if (items.length === 0) {
    return (
      <div className="border border-border p-2">
        <div className="mb-1 text-[12px] font-semibold">{title}</div>
        <StateEmpty message="No jobs in this group." />
      </div>
    );
  }
  return (
    <div className="border border-border p-2">
      <div className="mb-2 text-[12px] font-semibold">{title}</div>
      <div className="space-y-1.5">
        {items.map(({ job, step, stale }) => (
          <Link
            key={`${title}-${job.id}`}
            to={`/research/jobs/${encodeURIComponent(job.id)}`}
            className="block border border-border px-2 py-1.5 hover:bg-surface"
          >
            <div className="flex flex-wrap items-center gap-2">
              <StatusChip
                label={jobTypeLabel(job.job_type)}
                tone="neutral"
                compact
              />
              <StatusChip
                label={stale ? "Stale lease" : humanize(job.status)}
                tone={stale ? "danger" : jobTone(job.status)}
                compact
              />
            </div>
            <div className="mt-1 font-mono text-[11px] text-muted">
              {job.id}
            </div>
            {step && (
              <div className="mt-1 text-[11px] text-muted">
                Step {step.step_index + 1}: {humanize(step.name)}
              </div>
            )}
          </Link>
        ))}
      </div>
    </div>
  );
}

function QueueTransferGroup({
  title,
  items,
}: {
  title: string;
  items: ResearchQueueTransferItem[];
}) {
  if (items.length === 0) {
    return (
      <div>
        <div className="mb-1 text-[12px] font-semibold">{title}</div>
        <StateEmpty message="No transfers in this group." />
      </div>
    );
  }
  return (
    <div>
      <div className="mb-1 text-[12px] font-semibold">{title}</div>
      <div className="space-y-1.5">
        {items.map(({ transfer, stale }) => (
          <Link
            key={`${title}-${transfer.id}`}
            to={`/research/transfers/${encodeURIComponent(transfer.id)}`}
            className="block border border-border px-2 py-1.5 hover:bg-surface"
          >
            <div className="flex flex-wrap items-center gap-2">
              <StatusChip
                label={stale ? "Stale" : humanize(transfer.status)}
                tone={stale ? "danger" : transferTone(transfer.status)}
                compact
              />
              <span className="font-mono text-[11px] text-muted">
                {transfer.artifact_id}
              </span>
            </div>
            <div className="mt-1 font-mono text-[11px] text-muted">
              {transfer.source_machine_id ?? "-"} -&gt;{" "}
              {transfer.dest_machine_id ?? "-"}
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}

function TemplateRow({
  template,
  isAdmin,
  pending,
  onEdit,
  onAction,
}: {
  template: ResearchJobTemplate;
  isAdmin: boolean;
  pending: boolean;
  onEdit: () => void;
  onAction: (action: "archive" | "restore" | "delete") => void;
}) {
  return (
    <div className="border border-border px-2 py-2">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-semibold">{template.name}</span>
            <StatusChip
              label={jobTypeLabel(template.job_type)}
              tone="neutral"
              compact
            />
            <StatusChip
              label={humanize(template.status)}
              tone={template.status === "active" ? "success" : "muted"}
              compact
            />
          </div>
          <div className="mt-1 text-[11px] text-muted">
            Used {template.usage_count} time
            {template.usage_count === 1 ? "" : "s"}
            {template.last_used_at ? (
              <>
                {" "}
                · last used <RelativeTime epochMs={template.last_used_at} />
              </>
            ) : null}
          </div>
          {template.description && (
            <div className="mt-1 text-[12px] text-muted">
              {template.description}
            </div>
          )}
        </div>
        <div className="flex flex-wrap gap-1">
          <Button
            size="sm"
            aria-label={`Edit template ${template.name}`}
            disabled={!isAdmin || pending}
            onClick={onEdit}
          >
            Edit
          </Button>
          <Button
            size="sm"
            aria-label={`${template.status === "active" ? "Archive" : "Restore"} template ${template.name}`}
            disabled={!isAdmin || pending}
            onClick={() =>
              onAction(template.status === "active" ? "archive" : "restore")
            }
          >
            {template.status === "active" ? "Archive" : "Restore"}
          </Button>
          <Button
            size="sm"
            tone="danger"
            aria-label={`Delete template ${template.name}`}
            disabled={!isAdmin || pending}
            onClick={() => onAction("delete")}
          >
            Delete
          </Button>
        </div>
      </div>
    </div>
  );
}

function RetentionPanel({
  jobs,
  reports,
  artifacts,
  selection,
  setSelection,
}: {
  jobs: ResearchRetentionJobCandidate[];
  reports: ResearchRetentionReportCandidate[];
  artifacts: ResearchRetentionArtifactCandidate[];
  selection: RetentionSelection;
  setSelection: (selection: RetentionSelection) => void;
}) {
  return (
    <div className="space-y-3">
      <RetentionCandidateGroup
        title="Scratch DBs"
        empty="No completed job scratch candidates."
        rows={jobs}
        selectedIds={selection.jobIds}
        onSelect={(jobIds) => setSelection({ ...selection, jobIds })}
        idFor={(candidate) => candidate.job.id}
        labelFor={(candidate) => candidate.job.id}
        subFor={(candidate) =>
          `${jobTypeLabel(candidate.job.job_type)} · ${formatBytes(candidate.scratch_bytes)}`
        }
        eligibleFor={(candidate) => candidate.eligible}
        skipFor={(candidate) => candidate.skipped_reason}
      />
      <RetentionCandidateGroup
        title="Reports"
        empty="No report archive candidates."
        rows={reports}
        selectedIds={selection.reportIds}
        onSelect={(reportIds) => setSelection({ ...selection, reportIds })}
        idFor={(candidate) => candidate.report.id}
        labelFor={(candidate) => candidate.report.title}
        subFor={(candidate) => formatBytes(candidate.bytes)}
        eligibleFor={(candidate) => candidate.eligible}
        skipFor={(candidate) => candidate.skipped_reason}
      />
      <RetentionCandidateGroup
        title="Artifacts"
        empty="No artifact archive candidates."
        rows={artifacts}
        selectedIds={selection.artifactIds}
        onSelect={(artifactIds) =>
          setSelection({ ...selection, artifactIds })
        }
        idFor={(candidate) => candidate.artifact.id}
        labelFor={(candidate) => candidate.artifact.id}
        subFor={(candidate) =>
          `${formatBytes(candidate.bytes)} · ${candidate.active_dependency_count} active deps`
        }
        eligibleFor={(candidate) => candidate.eligible}
        skipFor={(candidate) => candidate.skipped_reason}
      />
    </div>
  );
}

function RetentionCandidateGroup<T>({
  title,
  empty,
  rows,
  selectedIds,
  onSelect,
  idFor,
  labelFor,
  subFor,
  eligibleFor,
  skipFor,
}: {
  title: string;
  empty: string;
  rows: T[];
  selectedIds: string[];
  onSelect: (ids: string[]) => void;
  idFor: (row: T) => string;
  labelFor: (row: T) => string;
  subFor: (row: T) => string;
  eligibleFor: (row: T) => boolean;
  skipFor: (row: T) => string | null;
}) {
  const eligibleRows = rows.filter(eligibleFor);
  if (eligibleRows.length === 0) {
    return (
      <div>
        <div className="mb-1 text-[12px] font-semibold">{title}</div>
        <StateEmpty message={empty} />
      </div>
    );
  }
  return (
    <div>
      <div className="mb-1 text-[12px] font-semibold">{title}</div>
      <div className="space-y-1.5">
        {rows.map((row) => {
          const id = idFor(row);
          const eligible = eligibleFor(row);
          const checked = selectedIds.includes(id);
          return (
            <label
              key={id}
              className="flex items-start gap-2 border border-border px-2 py-1.5 text-[12px]"
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={!eligible}
                onChange={(event) => {
                  const next = event.currentTarget.checked
                    ? [...selectedIds, id]
                    : selectedIds.filter((value) => value !== id);
                  onSelect(next);
                }}
                className="mt-0.5"
              />
              <span className="min-w-0">
                <span className="block truncate font-mono text-[11px]">
                  {labelFor(row)}
                </span>
                <span className="block text-[11px] text-muted">
                  {eligible ? subFor(row) : skipFor(row)}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </div>
  );
}

function RetentionResult({ result }: { result: RetentionArchiveResponse }) {
  const archived =
    result.jobs.filter((row) => row.status === "archived").length +
    result.reports.filter((row) => row.status === "archived").length +
    result.artifacts.filter((row) => row.status === "archived").length;
  const errors =
    result.jobs.filter((row) => row.status === "error").length +
    result.reports.filter((row) => row.status === "error").length +
    result.artifacts.filter((row) => row.status === "error").length;
  return (
    <Banner
      tone={errors > 0 ? "warning" : "success"}
      title="Retention archive complete"
    >
      Archived {archived} item{archived === 1 ? "" : "s"}; {errors} error
      {errors === 1 ? "" : "s"}.
    </Banner>
  );
}

function TemplateDialog({
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
      description="Templates are shared defaults for backtest and sweep jobs."
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
