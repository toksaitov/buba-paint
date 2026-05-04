import type { ReactNode } from "react";
import { useReducer, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useOutletContext } from "react-router-dom";
import { Check } from "lucide-react";
import { Loading } from "../components/common/loading";
import {
  AlertList,
  Banner,
  Button,
  CopyButton,
  FormField,
  Input,
  KeyValueList,
  ProgressBar,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
  Textarea,
} from "../components/ui/dashboard-primitives";
import {
  useLiveFills,
  useLiveControlAudit,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
} from "../hooks/use-live-status";
import { useTradingSummary } from "../hooks/use-trading-summary";
import { sendLiveControl } from "../lib/api";
import {
  capabilityForAction,
  cashHeadroom,
  controlActions,
  fingerprintPrefix,
  freshnessSignal,
  parseAuditDetails,
  type ControlActionConfig,
} from "../lib/live-controls";
import {
  healthTone,
  tradingStateLabel,
  tradingStateTone,
} from "../lib/trading-summary";
import { empty, help } from "../lib/copy";
import { useAuthStore } from "../stores/auth-store";
import { cn, formatDateTime, formatDurationShort, formatUsd, truncateMiddle } from "../lib/utils";
import type {
  LiveControlAction,
  LiveControlAuditRow,
  LiveFillRow,
  LiveOrderRow,
  LiveReconciliationRow,
  LiveRedemptionRow,
  RealAccountSummary,
  TradingControlCapability,
  TradingRiskSummary,
  TradingSummary,
} from "../lib/types";

const groupHeading: Record<"dryrun" | "reversible" | "destructive", string> = {
  dryrun: "Pre-trade Checks",
  reversible: "Session Lifecycle",
  destructive: "Destructive",
};

type ModeApplicability = "live" | "live_trading";

function modeMatches(target: ModeApplicability, current: string): boolean {
  if (target === "live") return current !== "paper";
  return current === "live_trading";
}

function modeChipLabel(target: ModeApplicability): string {
  return target === "live" ? "Live" : "Live trading";
}

function readableMode(current: string): string {
  if (current === "live_readonly") return "live readonly mode";
  if (current === "live_trading") return "live trading mode";
  return "paper mode";
}

interface ModeHeaderProps {
  title: string;
  applicability: ModeApplicability;
  current: string;
  titleClassName?: string;
}

function ModeHeader({ title, applicability, current, titleClassName }: ModeHeaderProps) {
  const matches = modeMatches(applicability, current);
  return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <h3 className={cn("font-semibold tracking-tight", titleClassName ?? "text-md")}>{title}</h3>
        <StatusChip
          label={modeChipLabel(applicability)}
          tone={matches ? "neutral" : "muted"}
          compact
        />
      </div>
      {!matches && (
        <p className="mt-0.5 text-xs text-muted">Inactive in {readableMode(current)}.</p>
      )}
    </div>
  );
}

const emptyRiskSummary: TradingRiskSummary = {
  halt_reason: null,
  halt_at_ms: null,
  high_water_mark: null,
  trough_equity: null,
  current_equity: null,
  daily_loss_usd: null,
  session_drawdown_usd: null,
  closeout_exported_at_ms: null,
};

function queryErrorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? `${fallback}: ${error.message}` : fallback;
}

function userStreamSummary(account: RealAccountSummary): string {
  const status = account.user_stream_status ?? "unknown";
  const eventAt = account.last_user_stream_event_at_ms;
  if (eventAt == null) {
    return status;
  }
  const age = Math.max(0, Date.now() - eventAt);
  return `${status} (${formatDurationShort(age)} ago)`;
}

function isMobileViewport() {
  return typeof window !== "undefined" && window.matchMedia?.("(max-width: 767px)").matches;
}

interface ActivityPanelProps {
  title: string;
  loading: boolean;
  error: unknown;
  loadingMessage: string;
  errorMessage: string;
  emptyMessage: string;
  count: number;
  children: ReactNode;
}

function ActivityPanel({
  title,
  loading,
  error,
  loadingMessage,
  errorMessage,
  emptyMessage,
  count,
  children,
}: ActivityPanelProps) {
  return (
    <div className="border-t border-border pt-3 first:border-t-0 first:pt-0">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="text-md font-semibold tracking-tight">{title}</div>
        {!loading && !error && count > 0 && (
          <div className="text-xs text-muted">{count} recent</div>
        )}
      </div>
      {loading ? (
        <StateEmpty message={loadingMessage} />
      ) : error ? (
        <div className="text-sm text-accent-blue">
          {queryErrorMessage(error, errorMessage)}
        </div>
      ) : count > 0 ? (
        <div className="divide-y divide-surface">{children}</div>
      ) : (
        <StateEmpty message={emptyMessage} />
      )}
    </div>
  );
}

function ActivityRow({
  title,
  subtitle,
  right,
}: {
  title: ReactNode;
  subtitle: ReactNode;
  right: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-3 py-2.5 text-sm">
      <div className="min-w-0">
        <div className="font-medium tracking-tight">{title}</div>
        <div className="mt-1 text-muted">{subtitle}</div>
      </div>
      <div className="shrink-0 text-right text-muted tabular-nums">{right}</div>
    </div>
  );
}

function liquidityBadge(side: string | null): string {
  if (side === "maker") return "M";
  if (side === "taker") return "T";
  return "";
}

function renderOrderRow(order: LiveOrderRow) {
  const priceLine =
    order.requested_price != null && order.limit_price != null
      ? order.requested_price === order.limit_price
        ? `req ${order.requested_price.toFixed(4)}`
        : `req ${order.requested_price.toFixed(4)} / lim ${order.limit_price.toFixed(4)}`
      : null;
  return (
    <ActivityRow
      key={order.id}
      title={order.status}
      subtitle={
        <>
          <div>{`${order.market_id} · ${order.side} · ${order.order_type}`}</div>
          {priceLine && <div className="mt-0.5 tabular-nums">{priceLine}</div>}
          {order.client_order_id && (
            <div className="mt-0.5 inline-flex items-center gap-1.5">
              <span className="text-muted/70">id</span>
              <code className="border border-border bg-surface px-1 py-0.5 tabular-nums">
                {truncateMiddle(order.client_order_id, 6, 6)}
              </code>
              <CopyButton size="sm" value={order.client_order_id} ariaLabel="Copy client order id" />
            </div>
          )}
          {order.status_reason && <div className="mt-0.5">{order.status_reason}</div>}
        </>
      }
      right={
        <div className="space-y-0.5">
          <div>{formatDateTime(order.updated_at_ms)}</div>
          {order.acknowledged_at_ms != null && (
            <div className="text-xs">
              ack <RelativeTime epochMs={order.acknowledged_at_ms} />
            </div>
          )}
        </div>
      }
    />
  );
}

function renderFillRow(fill: LiveFillRow) {
  const parts = [`${fill.size.toFixed(2)} @ ${fill.price.toFixed(4)}`];
  if (fill.fee_amount != null) {
    const rate = fill.fee_rate != null ? ` (${(fill.fee_rate * 100).toFixed(2)}%)` : "";
    parts.push(`fee ${formatUsd(fill.fee_amount)}${rate}`);
  }
  const badge = liquidityBadge(fill.liquidity_side);
  const title = badge ? `${fill.status} · ${badge}` : fill.status;
  return (
    <ActivityRow
      key={fill.id}
      title={title}
      subtitle={
        <>
          <div className="tabular-nums">{parts.join(" · ")}</div>
          {fill.tx_hash && (
            <div className="mt-0.5 inline-flex items-center gap-1.5">
              <span className="text-muted/70">tx</span>
              <code className="border border-border bg-surface px-1 py-0.5 tabular-nums">
                {truncateMiddle(fill.tx_hash, 6, 6)}
              </code>
              <CopyButton size="sm" value={fill.tx_hash} ariaLabel="Copy fill transaction hash" />
            </div>
          )}
        </>
      }
      right={formatDateTime(fill.filled_at_ms)}
    />
  );
}

function RedemptionDotline({ row }: { row: LiveRedemptionRow }) {
  const stages = [
    { key: "detected", filled: row.detected_redeemable_at_ms != null, title: "detected" },
    { key: "submitted", filled: row.submitted_at_ms != null, title: "submitted" },
    { key: "confirmed", filled: row.confirmed_at_ms != null, title: "confirmed" },
    { key: "credited", filled: row.cash_credit_observed_at_ms != null, title: "credited" },
  ];
  return (
    <span className="inline-flex items-center gap-1" aria-label="Redemption lifecycle">
      {stages.map((s) => (
        <span
          key={s.key}
          title={s.title}
          aria-label={`${s.title} ${s.filled ? "complete" : "pending"}`}
          className={cn(
            "h-1.5 w-1.5 border border-border",
            s.filled ? "bg-text" : "bg-bg",
          )}
        />
      ))}
    </span>
  );
}

function renderRedemptionRow(redemption: LiveRedemptionRow) {
  return (
    <ActivityRow
      key={redemption.id}
      title={redemption.status}
      subtitle={
        <>
          <div className="tabular-nums">
            {redemption.market_id} · {formatUsd(redemption.redeemable_value)}
          </div>
          <div className="mt-1.5 flex items-center gap-2">
            <RedemptionDotline row={redemption} />
            <span className="text-xs text-muted">
              {[
                redemption.detected_redeemable_at_ms != null ? "detected" : null,
                redemption.submitted_at_ms != null ? "submitted" : null,
                redemption.confirmed_at_ms != null ? "confirmed" : null,
                redemption.cash_credit_observed_at_ms != null ? "credited" : null,
              ]
                .filter((s): s is string => s != null)
                .join(" → ") || "pending"}
            </span>
          </div>
        </>
      }
      right={formatDateTime(redemption.detected_redeemable_at_ms)}
    />
  );
}

function renderReconciliationRow(event: LiveReconciliationRow) {
  const showDelta = event.local_value != null && event.remote_value != null;
  const delta = showDelta ? (event.local_value as number) - (event.remote_value as number) : null;
  return (
    <div key={event.id} className="flex items-start justify-between gap-3 py-2.5 text-sm">
      <div className="min-w-0">
        <StatusChip
          label={event.severity}
          tone={event.severity === "critical" ? "danger" : "warning"}
          compact
        />
        <div className="mt-2 font-medium tracking-tight">{event.event_type}</div>
        {showDelta && (
          <div className="mt-1 text-xs text-muted tabular-nums">
            local {formatUsd(event.local_value as number)} / remote {formatUsd(event.remote_value as number)} / Δ {formatUsd(delta as number)}
          </div>
        )}
      </div>
      <div className="shrink-0 text-right text-muted tabular-nums">{formatDateTime(event.timestamp_ms)}</div>
    </div>
  );
}

interface ActionResultState {
  status: "idle" | "pending" | "success" | "error";
  message: string | null;
  commandId: number | null;
  commandStatus: string | null;
  firedAtMs: number | null;
}

const initialActionResult: ActionResultState = {
  status: "idle",
  message: null,
  commandId: null,
  commandStatus: null,
  firedAtMs: null,
};

type ActionResultMap = Partial<Record<LiveControlAction, ActionResultState>>;

type ActionResultEvent =
  | { type: "fire"; action: LiveControlAction }
  | { type: "success"; action: LiveControlAction; commandId: number; commandStatus: string }
  | { type: "error"; action: LiveControlAction; message: string }
  | { type: "dismiss"; action: LiveControlAction };

function actionResultReducer(state: ActionResultMap, event: ActionResultEvent): ActionResultMap {
  switch (event.type) {
    case "fire":
      return {
        ...state,
        [event.action]: {
          status: "pending",
          message: null,
          commandId: null,
          commandStatus: null,
          firedAtMs: Date.now(),
        },
      };
    case "success":
      return {
        ...state,
        [event.action]: {
          status: "success",
          message: null,
          commandId: event.commandId,
          commandStatus: event.commandStatus,
          firedAtMs: Date.now(),
        },
      };
    case "error":
      return {
        ...state,
        [event.action]: {
          status: "error",
          message: event.message,
          commandId: null,
          commandStatus: null,
          firedAtMs: Date.now(),
        },
      };
    case "dismiss":
      return { ...state, [event.action]: initialActionResult };
  }
}

function ActionGate({
  capability,
  isAdmin,
  mobileBlocked,
}: {
  capability: TradingControlCapability;
  isAdmin: boolean;
  mobileBlocked: boolean;
}) {
  if (!isAdmin) {
    return <p className="text-xs text-accent-blue">Admin role required.</p>;
  }
  if (mobileBlocked) {
    return <p className="text-xs text-accent-blue">Desktop confirmation required.</p>;
  }
  return <p className="text-xs text-muted">{capability.reason}</p>;
}

interface ControlDraft {
  reason: string;
  confirmation: string;
  killCheckbox: boolean;
}

function emptyDraft(): ControlDraft {
  return { reason: "", confirmation: "", killCheckbox: false };
}

interface ActionRowProps {
  config: ControlActionConfig;
  capability: TradingControlCapability;
  draft: ControlDraft;
  expectedConfirmation: string | null;
  isAdmin: boolean;
  mobileBlocked: boolean;
  result: ActionResultState;
  onDraftChange: (action: LiveControlAction, next: ControlDraft) => void;
  onSubmit: (action: LiveControlAction) => void;
  onDismiss: (action: LiveControlAction) => void;
}

function ActionRow({
  config,
  capability,
  draft,
  expectedConfirmation,
  isAdmin,
  mobileBlocked,
  result,
  onDraftChange,
  onSubmit,
  onDismiss,
}: ActionRowProps) {
  const reasonOk = draft.reason.trim().length >= 5;
  const confirmationOk =
    expectedConfirmation == null || draft.confirmation === expectedConfirmation;
  const killCheckboxOk = !config.requiresExtraCheckbox || draft.killCheckbox;
  const disabled =
    !isAdmin ||
    !capability.enabled ||
    !reasonOk ||
    !confirmationOk ||
    !killCheckboxOk ||
    mobileBlocked ||
    result.status === "pending";
  const buttonState =
    result.status === "pending"
      ? "pending"
      : result.status === "success"
        ? "success"
        : result.status === "error"
          ? "error"
          : "idle";

  return (
    <div
      className="@container space-y-2 border-t border-border pt-3 first:border-t-0 first:pt-0"
      data-action={config.action}
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-md font-semibold tracking-tight">{config.label}</h3>
        <StatusChip
          label={capability.enabled ? "Available" : "Unavailable"}
          tone={capability.enabled ? "success" : "muted"}
          compact
        />
        <ActionGate capability={capability} isAdmin={isAdmin} mobileBlocked={mobileBlocked} />
      </div>

      <div className="grid gap-3 @3xl:grid-cols-[minmax(0,30rem)_minmax(16rem,1fr)] @3xl:gap-6">
        <div className="space-y-1.5 text-xs @3xl:order-2 @3xl:pt-[1.625rem]">
          <p className="text-text">{config.consequence}</p>
          <p className="text-muted">{config.effect}</p>
        </div>
        <div className="space-y-2 @3xl:order-1">
          <FormField label="Reason" hint="5 to 160 chars" required>
            {({ id, describedBy }) => (
              <Textarea
                id={id}
                aria-describedby={describedBy}
                placeholder='e.g. "manual gate refresh"'
                value={draft.reason}
                maxLength={160}
                minRows={2}
                spellCheck={false}
                onChange={(event) =>
                  onDraftChange(config.action, { ...draft, reason: event.target.value })
                }
              />
            )}
          </FormField>

          {expectedConfirmation && (
            <FormField label="Confirmation Phrase" hint="Type to match" required>
              {({ id, describedBy }) => (
                <div className="space-y-1.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <code className="border border-border bg-surface px-2 py-1 text-sm tabular-nums">
                      {expectedConfirmation}
                    </code>
                    <CopyButton
                      size="sm"
                      value={expectedConfirmation}
                      ariaLabel="Copy confirmation phrase"
                    />
                  </div>
                  <Input
                    id={id}
                    aria-describedby={describedBy}
                    value={draft.confirmation}
                    autoCorrect="off"
                    autoCapitalize="off"
                    spellCheck={false}
                    state={
                      draft.confirmation.length === 0
                        ? "idle"
                        : confirmationOk
                          ? "valid"
                          : "invalid"
                    }
                    onChange={(event) =>
                      onDraftChange(config.action, { ...draft, confirmation: event.target.value })
                    }
                  />
                  {confirmationOk && draft.confirmation.length > 0 && (
                    <p className="inline-flex items-center gap-1 text-xs text-accent-green">
                      <Check size={12} aria-hidden /> Match
                    </p>
                  )}
                </div>
              )}
            </FormField>
          )}

          {config.requiresExtraCheckbox && (
            <label className="inline-flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={draft.killCheckbox}
                onChange={(event) =>
                  onDraftChange(config.action, { ...draft, killCheckbox: event.target.checked })
                }
              />
              <span>Acknowledge: this halts the session.</span>
            </label>
          )}

          <div className="flex justify-end pt-1">
            <Button
              tone={config.tone === "danger" ? "danger" : "neutral"}
              state={buttonState}
              disabled={disabled}
              onClick={() => onSubmit(config.action)}
              className="w-full @3xl:w-auto @3xl:min-w-[140px]"
            >
              {config.label}
            </Button>
          </div>
        </div>
      </div>

      {result.status === "error" && result.message && (
        <Banner
          tone="danger"
          title="Command Failed"
          onDismiss={() => onDismiss(config.action)}
        >
          {result.message}
        </Banner>
      )}
      {result.status === "success" && result.commandId != null && (
        <Banner
          tone="success"
          title={`Command #${result.commandId} queued as ${result.commandStatus ?? "pending"}.`}
          onDismiss={() => onDismiss(config.action)}
        >
          {result.firedAtMs != null && <RelativeTime epochMs={result.firedAtMs} />}
        </Banner>
      )}
    </div>
  );
}

const groupApplicability: Record<
  "dryrun" | "reversible" | "destructive",
  ModeApplicability
> = {
  dryrun: "live",
  reversible: "live_trading",
  destructive: "live_trading",
};

interface ActionGroupProps {
  group: "dryrun" | "reversible" | "destructive";
  actions: ControlActionConfig[];
  currentMode: string;
  children: (config: ControlActionConfig) => ReactNode;
}

function ActionGroup({ group, actions, currentMode, children }: ActionGroupProps) {
  const heading = groupHeading[group];
  const applicability = groupApplicability[group];
  const matches = modeMatches(applicability, currentMode);
  return (
    <section
      className={cn(
        "space-y-3",
        group === "destructive" && "border border-accent-red bg-[var(--color-danger-fill)] p-3",
        !matches && "opacity-75",
      )}
      data-group={group}
    >
      <ModeHeader title={heading} applicability={applicability} current={currentMode} />
      <div className="space-y-3">{actions.map((config) => children(config))}</div>
    </section>
  );
}

interface RecentAuditStripProps {
  entries: LiveControlAuditRow[];
  loading: boolean;
}

function RecentAuditStrip({ entries, loading }: RecentAuditStripProps) {
  if (loading && entries.length === 0) {
    return (
      <div className="space-y-1 border border-border bg-surface px-3 py-2">
        <div className="text-md font-semibold tracking-tight">Recent Commands</div>
        <StateEmpty message="Loading recent commands" />
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <div className="space-y-1 border border-border bg-surface px-3 py-2">
        <div className="text-md font-semibold tracking-tight">Recent Commands</div>
        <StateEmpty message="No recent commands." />
      </div>
    );
  }
  const recent = entries.slice(0, 3);
  return (
    <div className="space-y-1 border border-border bg-surface px-3 py-2">
      <div className="text-md font-semibold tracking-tight">Recent Commands</div>
      <ul className="divide-y divide-border">
        {recent.map((entry) => {
          const parsed = parseAuditDetails(entry.details_json);
          const tone =
            parsed.status === "applied" || parsed.status === "success"
              ? "success"
              : parsed.status === "failed" || parsed.status === "error"
                ? "danger"
                : "muted";
          return (
            <li key={entry.id} className="flex items-center justify-between gap-3 py-1.5 text-xs">
              <RelativeTime epochMs={entry.timestamp_ms} className="shrink-0" />
              <span className="min-w-0 flex-1 truncate text-muted" title={entry.actor}>
                {entry.actor}
              </span>
              <StatusChip label={parsed.command ?? entry.action} tone={tone} compact />
            </li>
          );
        })}
      </ul>
    </div>
  );
}

interface CashHeadroomDeckProps {
  account: RealAccountSummary;
}

function CashHeadroomDeck({ account }: CashHeadroomDeckProps) {
  const head = cashHeadroom(account);
  if (head == null) return null;
  const labelTone =
    head.tone === "danger"
      ? "text-accent-red"
      : head.tone === "warning"
        ? "text-accent-blue"
        : "text-text";
  return (
    <div className="space-y-1.5 border border-border bg-bg p-3">
      <div className="flex items-center justify-between gap-3 text-sm">
        <span className="text-muted">Cash in Play</span>
        <div className={cn("tabular-nums", labelTone)}>
          {formatUsd(head.inPlay)} of {formatUsd(head.cap)} ({(head.fraction * 100).toFixed(1)}%)
        </div>
      </div>
      <ProgressBar
        value={head.fraction}
        ariaLabel={`Cash in Play, ${(head.fraction * 100).toFixed(1)}% of cap`}
      />
      <div className="flex items-center justify-between gap-3 text-xs text-muted">
        <span>reserved + inventory mark + pending redeem</span>
        <span>{formatUsd(head.available)} available</span>
      </div>
    </div>
  );
}

interface ControlDeckProps {
  botId: string;
  summary: TradingSummary;
  fingerprintPrefixValue: string;
  isAdmin: boolean;
  auditEntries: LiveControlAuditRow[];
  auditLoading: boolean;
}

function ControlDeck({
  botId,
  summary,
  fingerprintPrefixValue,
  isAdmin,
  auditEntries,
  auditLoading,
}: ControlDeckProps) {
  const queryClient = useQueryClient();
  const [drafts, setDrafts] = useState<Partial<Record<LiveControlAction, ControlDraft>>>({});
  const [results, dispatch] = useReducer(actionResultReducer, {} as ActionResultMap);
  const mutation = useMutation({
    mutationFn: (vars: { action: LiveControlAction; draft: ControlDraft }) =>
      sendLiveControl(botId, {
        action: vars.action,
        reason: vars.draft.reason,
        confirmation: vars.draft.confirmation || undefined,
      }),
    onMutate: (vars) => {
      dispatch({ type: "fire", action: vars.action });
    },
    onSuccess: (data, vars) => {
      dispatch({
        type: "success",
        action: vars.action,
        commandId: data.command_id,
        commandStatus: data.status,
      });
      void queryClient.invalidateQueries({ queryKey: ["trading-summary", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-control-audit", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-sessions", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-orders", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-fills", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-redemptions", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-reconciliation", botId] });
    },
    onError: (error, vars) => {
      dispatch({
        type: "error",
        action: vars.action,
        message: error instanceof Error && error.message ? error.message : "Control request failed",
      });
    },
  });

  const visibleActions = controlActions;
  const deckTitle = "Controls";

  if (visibleActions.length === 0) {
    return (
      <SectionCard title={deckTitle}>
        <StateEmpty message="No dashboard controls are available for this session state." />
      </SectionCard>
    );
  }

  const mobile = isMobileViewport();
  const groupOrder: Array<"dryrun" | "reversible" | "destructive"> = [
    "dryrun",
    "reversible",
    "destructive",
  ];
  const grouped = new Map<"dryrun" | "reversible" | "destructive", ControlActionConfig[]>();
  for (const action of visibleActions) {
    const arr = grouped.get(action.group) ?? [];
    arr.push(action);
    grouped.set(action.group, arr);
  }

  function handleDraftChange(action: LiveControlAction, next: ControlDraft) {
    setDrafts((current) => ({ ...current, [action]: next }));
  }

  function handleSubmit(action: LiveControlAction) {
    const draft = drafts[action] ?? emptyDraft();
    mutation.mutate({ action, draft });
  }

  function handleDismiss(action: LiveControlAction) {
    dispatch({ type: "dismiss", action });
  }

  return (
    <SectionCard
      title={deckTitle}
      toolbar={
        <StatusChip
          label={tradingStateLabel(summary.trading_state)}
          tone={tradingStateTone(summary.trading_state)}
          compact
          help={summary.trading_state === "readonly" ? help.readonly : undefined}
        />
      }
    >
      <div className="space-y-4">
        <RecentAuditStrip entries={auditEntries} loading={auditLoading} />
        {groupOrder.map((group) => {
          const actions = grouped.get(group);
          if (!actions || actions.length === 0) return null;
          return (
            <ActionGroup
              key={group}
              group={group}
              actions={actions}
              currentMode={summary.runtime_mode}
            >
              {(config) => {
                const capability = capabilityForAction(summary.capabilities, config.action);
                const draft = drafts[config.action] ?? emptyDraft();
                const expected =
                  config.confirmation?.(botId, fingerprintPrefixValue) ?? null;
                const mobileBlocked = Boolean(config.desktopOnly && mobile);
                const result = results[config.action] ?? initialActionResult;
                return (
                  <ActionRow
                    key={config.action}
                    config={config}
                    capability={capability}
                    draft={draft}
                    expectedConfirmation={expected}
                    isAdmin={isAdmin}
                    mobileBlocked={mobileBlocked}
                    result={result}
                    onDraftChange={handleDraftChange}
                    onSubmit={handleSubmit}
                    onDismiss={handleDismiss}
                  />
                );
              }}
            </ActionGroup>
          );
        })}
      </div>
    </SectionCard>
  );
}

interface StatusBannerProps {
  summary: TradingSummary;
  risk: TradingRiskSummary;
}

function StatusBanner({ summary, risk }: StatusBannerProps) {
  const state = summary.trading_state;
  if (state === "halted") {
    return (
      <Banner
        tone="danger"
        title="Session Halted"
        role="alert"
        ariaLive="assertive"
      >
        <div className="space-y-3">
          <p>
            This session cannot be re-armed from the dashboard. Export the run, analyze the halt,
            and start a new session only after a separate operator decision.
          </p>
          <KeyValueList
            columns={2}
            items={[
              {
                label: "Halt reason",
                value: risk.halt_reason ?? "n/a",
                tone: "danger",
              },
              {
                label: "Halt time",
                value: risk.halt_at_ms != null ? formatDateTime(risk.halt_at_ms) : "n/a",
              },
              {
                label: "Current equity",
                value: risk.current_equity != null ? formatUsd(risk.current_equity) : "n/a",
              },
              {
                label: "Session drawdown",
                value:
                  risk.session_drawdown_usd != null
                    ? formatUsd(risk.session_drawdown_usd)
                    : "n/a",
                tone: "danger",
              },
              {
                label: "Daily loss",
                value: risk.daily_loss_usd != null ? formatUsd(risk.daily_loss_usd) : "n/a",
                tone: "danger",
              },
              {
                label: "Closeout",
                value:
                  risk.closeout_exported_at_ms != null
                    ? formatDateTime(risk.closeout_exported_at_ms)
                    : "not exported",
              },
            ]}
          />
        </div>
      </Banner>
    );
  }
  if (state === "unknown_order") {
    return (
      <Banner
        tone="danger"
        title="Unknown Order State"
        role="alert"
        ariaLive="assertive"
      >
        New submissions stay blocked until venue orders, fills, and account state are reconciled.
      </Banner>
    );
  }
  return null;
}

interface StaleDataBannerProps {
  account: RealAccountSummary;
}

function StaleDataBanner({ account }: StaleDataBannerProps) {
  const fresh = freshnessSignal(account);
  if (!fresh.stale) return null;
  const reasons: string[] = [];
  if (fresh.userStreamStale) {
    reasons.push(
      fresh.userStreamStatus && fresh.userStreamStatus !== "ok"
        ? `user stream ${fresh.userStreamStatus}`
        : "user stream lagging",
    );
  }
  if (fresh.refreshStale) {
    reasons.push("account refresh lagging");
  }
  return (
    <Banner tone="warning" title="Live State May Be Stale">
      {reasons.join(" / ")}.
    </Banner>
  );
}

interface AccountSectionProps {
  summary: TradingSummary;
  account: RealAccountSummary;
}

function AccountSection({ summary, account }: AccountSectionProps) {
  const active = modeMatches("live", summary.runtime_mode);
  const head = cashHeadroom(account);
  const body = (
    <div className="space-y-3">
      {active && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-muted">Health</span>
          <StatusChip
            label={summary.venue_health.label}
            tone={healthTone(summary.venue_health)}
            compact
            title={summary.venue_health.detail ?? "Venue feeds"}
          />
          <StatusChip
            label={summary.account_health.label}
            tone={healthTone(summary.account_health)}
            compact
            title={summary.account_health.detail ?? "Account snapshot"}
          />
          <StatusChip
            label={summary.reconciliation_health.label}
            tone={healthTone(summary.reconciliation_health)}
            compact
            title={summary.reconciliation_health.detail ?? "Local vs remote reconciliation"}
          />
        </div>
      )}
      {head != null && <CashHeadroomDeck account={account} />}
      <KeyValueList
          columns={2}
          items={[
            {
              label: "Cash available",
              value:
                account.available_cash != null ? formatUsd(account.available_cash) : "n/a",
              help: help.cashAvailable,
            },
            {
              label: "Total equity",
              value: account.total_equity != null ? formatUsd(account.total_equity) : "n/a",
            },
            {
              label: "Reserved",
              value:
                account.reserved_cash != null ? formatUsd(account.reserved_cash) : "n/a",
              help: help.reserved,
            },
            {
              label: "Inventory mark",
              value:
                account.inventory_mark_value != null
                  ? formatUsd(account.inventory_mark_value)
                  : "n/a",
              help: help.inventoryMark,
            },
            {
              label: "Redeemable",
              value:
                account.redeemable_value != null
                  ? formatUsd(account.redeemable_value)
                  : "n/a",
              help: help.redeemable,
            },
            {
              label: "Pending redeem",
              value:
                account.pending_redeem_value != null
                  ? formatUsd(account.pending_redeem_value)
                  : "n/a",
              help: help.pendingRedeem,
            },
            {
              label: "Allowance",
              value:
                account.allowance_available != null
                  ? formatUsd(account.allowance_available)
                  : "n/a",
              help: help.allowance,
            },
            {
              label: "Open orders",
              value: active ? account.open_orders.toString() : "n/a",
            },
            {
              label: "User stream",
              value: active ? userStreamSummary(account) : "n/a",
            },
            {
              label: "Last refresh",
              value:
                account.last_account_refresh_at_ms != null
                  ? formatDateTime(account.last_account_refresh_at_ms)
                  : "n/a",
            },
          ]}
        />
      </div>
  );

  if (active) {
    return (
      <SectionCard
        title="Polymarket Account"
        toolbar={<StatusChip label="Live" tone="neutral" compact />}
      >
        {body}
      </SectionCard>
    );
  }

  return (
    <details className="border border-border bg-bg">
      <summary className="cursor-pointer px-3 py-2.5">
        <span className="inline-flex items-baseline gap-2 flex-wrap">
          <span className="text-[14px] font-semibold tracking-tight">Polymarket Account</span>
          <StatusChip label="Live" tone="muted" compact />
          <span className="text-[11px] text-muted">
            Inactive in {readableMode(summary.runtime_mode)}.
          </span>
        </span>
      </summary>
      <div className="border-t border-border p-3 opacity-75">{body}</div>
    </details>
  );
}

export function ExecutionPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const user = useAuthStore((state) => state.user);
  const { data: summary, isLoading } = useTradingSummary(botId);
  const isPaper = summary?.runtime_mode === "paper";
  const liveDetailsEnabled = !!summary && !isPaper;
  const sessionsQuery = useLiveSessions(botId, 6, liveDetailsEnabled);
  const ordersQuery = useLiveOrders(botId, 6, liveDetailsEnabled);
  const fillsQuery = useLiveFills(botId, 6, liveDetailsEnabled);
  const redemptionsQuery = useLiveRedemptions(botId, 6, liveDetailsEnabled);
  const reconciliationQuery = useLiveReconciliation(botId, 6, liveDetailsEnabled);
  const controlAuditQuery = useLiveControlAudit(botId, 8, liveDetailsEnabled);

  if (isLoading || !summary) return <Loading label="Loading execution" />;

  const account = summary.real_account_summary;
  const risk = summary.risk_summary ?? emptyRiskSummary;
  const latestSession = sessionsQuery.data?.sessions[0] ?? null;
  const isAdmin = user?.role === "admin";
  const detailQueryFailures = liveDetailsEnabled
    ? [
        sessionsQuery.isError ? "Session details" : null,
        ordersQuery.isError ? "Orders" : null,
        fillsQuery.isError ? "Fills" : null,
        redemptionsQuery.isError ? "Redemptions" : null,
        reconciliationQuery.isError ? "Reconciliation" : null,
        controlAuditQuery.isError ? "Control audit" : null,
      ].filter((panel): panel is string => panel != null)
    : [];
  const detailPanelsDegraded = detailQueryFailures.length > 0;

  return (
    <div className="space-y-3">
      <StatusBanner summary={summary} risk={risk} />
      <StaleDataBanner account={account} />

      {detailPanelsDegraded && (
        <Banner tone="warning" title="Detail Panels Degraded">
          Summary is current, but some execution detail panels are unavailable:{" "}
          {detailQueryFailures.join(", ")}.
        </Banner>
      )}

      {summary.alerts.length > 0 && (
        <SectionCard title="Alerts">
          <AlertList alerts={summary.alerts} />
        </SectionCard>
      )}

      <ControlDeck
        botId={botId}
        summary={summary}
        fingerprintPrefixValue={fingerprintPrefix(latestSession)}
        isAdmin={isAdmin}
        auditEntries={controlAuditQuery.data?.entries ?? []}
        auditLoading={controlAuditQuery.isLoading}
      />

      <AccountSection summary={summary} account={account} />

      {(() => {
        const venueActive = modeMatches("live", summary.runtime_mode);
        const venueBody = (
          <div className="grid gap-4 xl:grid-cols-2">
            <ActivityPanel
              title="Orders"
              loading={ordersQuery.isLoading}
              error={ordersQuery.isError ? ordersQuery.error : null}
              loadingMessage="Loading venue orders..."
              errorMessage="Venue orders are currently unavailable"
              emptyMessage={empty.noVenueOrders}
              count={ordersQuery.data?.orders.length ?? 0}
            >
              {ordersQuery.data?.orders.map(renderOrderRow)}
            </ActivityPanel>
            <ActivityPanel
              title="Fills"
              loading={fillsQuery.isLoading}
              error={fillsQuery.isError ? fillsQuery.error : null}
              loadingMessage="Loading venue fills..."
              errorMessage="Venue fills are currently unavailable"
              emptyMessage={empty.noVenueFills}
              count={fillsQuery.data?.fills.length ?? 0}
            >
              {fillsQuery.data?.fills.map(renderFillRow)}
            </ActivityPanel>
            <ActivityPanel
              title="Redemptions"
              loading={redemptionsQuery.isLoading}
              error={redemptionsQuery.isError ? redemptionsQuery.error : null}
              loadingMessage="Loading redemptions..."
              errorMessage="Redemption details are currently unavailable"
              emptyMessage={empty.noRedemptions}
              count={redemptionsQuery.data?.redemptions.length ?? 0}
            >
              {redemptionsQuery.data?.redemptions.map(renderRedemptionRow)}
            </ActivityPanel>
            <ActivityPanel
              title="Reconciliation"
              loading={reconciliationQuery.isLoading}
              error={reconciliationQuery.isError ? reconciliationQuery.error : null}
              loadingMessage="Loading reconciliation..."
              errorMessage="Reconciliation events are currently unavailable"
              emptyMessage={empty.noReconciliation}
              count={reconciliationQuery.data?.events.length ?? 0}
            >
              {reconciliationQuery.data?.events.map(renderReconciliationRow)}
            </ActivityPanel>
          </div>
        );
        if (venueActive) {
          return (
            <SectionCard
              title="Venue Activity"
              toolbar={<StatusChip label="Live" tone="neutral" compact />}
            >
              {venueBody}
            </SectionCard>
          );
        }
        return (
          <details className="border border-border bg-bg">
            <summary className="cursor-pointer px-3 py-2.5">
              <span className="inline-flex items-baseline gap-2 flex-wrap">
                <span className="text-[14px] font-semibold tracking-tight">Venue Activity</span>
                <StatusChip label="Live" tone="muted" compact />
                <span className="text-[11px] text-muted">
                  Inactive in {readableMode(summary.runtime_mode)}.
                </span>
              </span>
            </summary>
            <div className="border-t border-border p-3 opacity-75">{venueBody}</div>
          </details>
        );
      })()}

      <details className="border border-border bg-bg">
        <summary className="cursor-pointer px-3 py-2.5">
          <span className="inline-flex items-baseline gap-2 flex-wrap">
            <span className="text-[14px] font-semibold tracking-tight">Session and Wallet</span>
            <StatusChip
              label="Live"
              tone={modeMatches("live", summary.runtime_mode) ? "neutral" : "muted"}
              compact
            />
            {!modeMatches("live", summary.runtime_mode) && (
              <span className="text-[11px] text-muted">
                Inactive in {readableMode(summary.runtime_mode)}.
              </span>
            )}
          </span>
        </summary>
        <div
          className={cn(
            "border-t border-border p-3",
            !modeMatches("live", summary.runtime_mode) && "opacity-75",
          )}
        >
          {sessionsQuery.isError && (
            <div className="mb-3 text-sm text-accent-blue">
              {queryErrorMessage(
                sessionsQuery.error,
                "Live session details are currently unavailable",
              )}
            </div>
          )}
          <KeyValueList
            columns={2}
            items={[
              {
                label: "Session",
                value: account.session_id != null ? `#${account.session_id}` : "n/a",
              },
              { label: "Session status", value: account.session_status ?? "n/a" },
              {
                label: "Started",
                value:
                  account.session_started_at_ms != null
                    ? formatDateTime(account.session_started_at_ms)
                    : "n/a",
              },
              {
                label: "Cash cap",
                value: account.cash_cap_usd != null ? formatUsd(account.cash_cap_usd) : "n/a",
                help: help.cashCap,
              },
              {
                label: "Strategies",
                value:
                  account.enabled_strategies.length > 0
                    ? account.enabled_strategies.join(", ")
                    : "n/a",
              },
              {
                label: "Wallet",
                value: account.wallet_address ? truncateMiddle(account.wallet_address) : "n/a",
              },
              {
                label: "Proxy",
                value: account.proxy_wallet ? truncateMiddle(account.proxy_wallet) : "n/a",
                help: help.proxy,
              },
              {
                label: "Session fingerprint",
                value:
                  latestSession?.config_fingerprint != null
                    ? truncateMiddle(latestSession.config_fingerprint, 12, 12)
                    : "n/a",
                help: help.sessionFingerprint,
              },
            ]}
          />
        </div>
      </details>
    </div>
  );
}
