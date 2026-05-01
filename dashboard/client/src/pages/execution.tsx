import type { ReactNode } from "react";
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import {
  AlertList,
  InfoHint,
  KeyValueList,
  SectionCard,
  StateEmpty,
  StatusChip,
  Surface,
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
  healthTone,
  processStateLabel,
  processStateTone,
  runtimeModeLabel,
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
  TradingCapabilities,
  TradingSummary,
} from "../lib/types";

function capabilityTone(enabled: boolean) {
  return enabled ? "success" : "muted";
}

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

function ValueLine({
  label,
  value,
  tone,
  detail,
}: {
  label: string;
  value: ReactNode;
  tone?: "success" | "danger" | "warning";
  detail?: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-surface py-2 text-[12px] last:border-0">
      <span className="text-muted">{label}</span>
      <span className="flex max-w-[70%] flex-col items-end gap-0.5 text-right tabular-nums">
        <span
          className={cn(
            tone === "success" && "text-accent-green",
            tone === "danger" && "text-accent-red",
            tone === "warning" && "text-accent-blue",
          )}
        >
          {value}
        </span>
        {detail && <span className="text-[11px] text-muted">{detail}</span>}
      </span>
    </div>
  );
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
        <div className="text-[13px] font-semibold tracking-tight">{title}</div>
        {!loading && !error && count > 0 && (
          <div className="text-[11px] text-muted">{count} recent</div>
        )}
      </div>
      {loading ? (
        <StateEmpty message={loadingMessage} />
      ) : error ? (
        <div className="text-[12px] text-accent-blue">
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
    <div className="flex items-start justify-between gap-3 py-2.5 text-[12px]">
      <div className="min-w-0">
        <div className="font-medium tracking-tight">{title}</div>
        <div className="mt-1 text-muted">{subtitle}</div>
      </div>
      <div className="shrink-0 text-right text-muted">{right}</div>
    </div>
  );
}

function renderOrderRow(order: LiveOrderRow) {
  return (
    <ActivityRow
      key={order.id}
      title={order.status}
      subtitle={
        <>
          <div>{`${order.market_id} · ${order.side} · ${order.order_type}`}</div>
          {order.status_reason && <div className="mt-0.5">{order.status_reason}</div>}
        </>
      }
      right={formatDateTime(order.updated_at_ms)}
    />
  );
}

function liquidityBadge(side: string | null): string {
  if (side === "maker") return "M";
  if (side === "taker") return "T";
  return "";
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
      subtitle={parts.join(" · ")}
      right={formatDateTime(fill.filled_at_ms)}
    />
  );
}

function renderRedemptionRow(redemption: LiveRedemptionRow) {
  return (
    <ActivityRow
      key={redemption.id}
      title={redemption.status}
      subtitle={`${redemption.market_id} · ${formatUsd(redemption.redeemable_value)}`}
      right={formatDateTime(redemption.detected_redeemable_at_ms)}
    />
  );
}

function renderReconciliationRow(event: LiveReconciliationRow) {
  return (
    <div key={event.id} className="flex items-start justify-between gap-3 py-2.5 text-[12px]">
      <div className="min-w-0">
        <StatusChip
          label={event.severity}
          tone={event.severity === "critical" ? "danger" : "warning"}
          compact
        />
        <div className="mt-2 font-medium tracking-tight">{event.event_type}</div>
      </div>
      <div className="shrink-0 text-right text-muted">{formatDateTime(event.timestamp_ms)}</div>
    </div>
  );
}

const capabilityHelp: Record<string, string> = {
  preflight: help.preflight,
  arm: help.arm,
  disarm: help.disarm,
  stop_after_flat: help.stopAfterFlat,
  kill_switch: help.killSwitch,
};

interface ControlActionConfig {
  action: LiveControlAction;
  label: string;
  tone: "neutral" | "danger";
  confirmation?: (botId: string, fingerprintPrefix: string) => string;
  desktopOnly?: boolean;
}

interface ControlDraft {
  reason: string;
  confirmation: string;
}

const controlActions: ControlActionConfig[] = [
  { action: "preflight", label: "Preflight", tone: "neutral" },
  {
    action: "arm",
    label: "Arm",
    tone: "danger",
    desktopOnly: true,
    confirmation: (botId, fingerprintPrefix) => `ARM ${botId} ${fingerprintPrefix}`,
  },
  { action: "disarm", label: "Disarm", tone: "neutral" },
  { action: "stop_after_flat", label: "Stop after flat", tone: "neutral" },
  {
    action: "cancel_all",
    label: "Cancel all",
    tone: "danger",
    confirmation: (botId) => `CANCEL ALL ${botId}`,
  },
  {
    action: "redeem_all",
    label: "Redeem all",
    tone: "danger",
    desktopOnly: true,
    confirmation: (botId) => `REDEEM ALL ${botId}`,
  },
  {
    action: "kill_switch",
    label: "Kill switch",
    tone: "danger",
    confirmation: (botId) => `KILL ${botId}`,
  },
];

function visibleControlActions(tradingState: string): ControlActionConfig[] {
  const actions = new Set<LiveControlAction>(
    tradingState === "disarmed"
      ? ["preflight", "arm"]
      : tradingState === "armed"
        ? ["disarm", "stop_after_flat", "cancel_all", "redeem_all", "kill_switch"]
        : tradingState === "stop_after_flat"
          ? ["disarm", "cancel_all", "redeem_all", "kill_switch"]
          : tradingState === "unknown_order"
            ? ["preflight", "cancel_all", "kill_switch"]
            : tradingState === "halted"
              ? []
              : ["preflight", "kill_switch"],
  );
  return controlActions.filter((config) => actions.has(config.action));
}

function capabilityForAction(capabilities: TradingCapabilities, action: LiveControlAction) {
  if (action === "redeem_all") return capabilities.redeem;
  return capabilities[action];
}

function fingerprintPrefix(session: { config_fingerprint: string } | null): string {
  return (session?.config_fingerprint ?? "").slice(0, 12);
}

function isMobileViewport() {
  return typeof window !== "undefined" && window.matchMedia?.("(max-width: 767px)").matches;
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
    return <p className="text-[11px] text-accent-blue">Admin role required.</p>;
  }
  if (mobileBlocked) {
    return <p className="text-[11px] text-accent-blue">Desktop confirmation required.</p>;
  }
  if (!capability.enabled) {
    return <p className="text-[11px] text-muted">{capability.reason}</p>;
  }
  return <p className="text-[11px] text-muted">{capability.reason}</p>;
}

function ControlCommandPanel({
  botId,
  summary,
  latestSession,
  isAdmin,
}: {
  botId: string;
  summary: TradingSummary;
  latestSession: { config_fingerprint: string } | null;
  isAdmin: boolean;
}) {
  const queryClient = useQueryClient();
  const [drafts, setDrafts] = useState<Record<LiveControlAction, ControlDraft>>({
    preflight: { reason: "", confirmation: "" },
    arm: { reason: "", confirmation: "" },
    disarm: { reason: "", confirmation: "" },
    stop_after_flat: { reason: "", confirmation: "" },
    cancel_all: { reason: "", confirmation: "" },
    redeem_all: { reason: "", confirmation: "" },
    kill_switch: { reason: "", confirmation: "" },
  });
  const mutation = useMutation({
    mutationFn: ({
      action,
      draft,
    }: {
      action: LiveControlAction;
      draft: ControlDraft;
    }) =>
      sendLiveControl(botId, {
        action,
        reason: draft.reason,
        confirmation: draft.confirmation || undefined,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["trading-summary", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-control-audit", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-sessions", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-orders", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-fills", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-redemptions", botId] });
      void queryClient.invalidateQueries({ queryKey: ["live-reconciliation", botId] });
    },
  });

  if (summary.runtime_mode === "live_readonly") {
    return <StateEmpty message="Readonly mode observes venue truth but cannot queue controls." />;
  }

  const fpPrefix = fingerprintPrefix(latestSession);
  const mobile = isMobileViewport();
  const visibleActions = visibleControlActions(summary.trading_state);

  if (visibleActions.length === 0) {
    return <StateEmpty message="No dashboard controls are available for this session state." />;
  }

  return (
    <div className="divide-y divide-surface">
      {visibleActions.map((config) => {
        const capability = capabilityForAction(summary.capabilities, config.action);
        const draft = drafts[config.action];
        const expected = config.confirmation?.(botId, fpPrefix);
        const needsConfirmation = expected != null;
        const reasonOk = draft.reason.trim().length > 0;
        const confirmationOk = !needsConfirmation || draft.confirmation === expected;
        const mobileBlocked = Boolean(config.desktopOnly && mobile);
        const disabled =
          !isAdmin ||
          !capability.enabled ||
          !reasonOk ||
          !confirmationOk ||
          mobileBlocked ||
          mutation.isPending;
        const termHelp = capabilityHelp[config.action];
        return (
          <div key={config.action} className="grid gap-3 py-3 lg:grid-cols-[1fr_220px]">
            <div className="min-w-0 space-y-2">
              <div className="flex flex-wrap items-center gap-2">
                <div className="flex items-center gap-1 text-[13px] font-semibold tracking-tight">
                  <span>{config.label}</span>
                  {termHelp && <InfoHint label={config.label} text={termHelp} />}
                </div>
                <StatusChip
                  label={capability.enabled ? "Available" : "Unavailable"}
                  tone={capabilityTone(capability.enabled)}
                  compact
                  help={capability.enabled ? undefined : help.unavailable}
                />
              </div>
              <ActionGate
                capability={capability}
                isAdmin={isAdmin}
                mobileBlocked={mobileBlocked}
              />
              <input
                className="w-full border border-border bg-bg px-2 py-1.5 text-[12px] outline-none focus:border-text"
                placeholder="Reason"
                value={draft.reason}
                onChange={(event) =>
                  setDrafts((current) => ({
                    ...current,
                    [config.action]: { ...current[config.action], reason: event.target.value },
                  }))
                }
              />
              {expected && (
                <div className="space-y-1">
                  <div className="text-[11px] text-muted">
                    Type <span className="text-text">{expected}</span>
                  </div>
                  <input
                    className="w-full border border-border bg-bg px-2 py-1.5 text-[12px] outline-none focus:border-text"
                    value={draft.confirmation}
                    onChange={(event) =>
                      setDrafts((current) => ({
                        ...current,
                        [config.action]: {
                          ...current[config.action],
                          confirmation: event.target.value,
                        },
                      }))
                    }
                  />
                </div>
              )}
            </div>
            <div className="flex items-end justify-start lg:justify-end">
              <button
                className={cn(
                  "border px-3 py-2 text-[12px] font-semibold tracking-tight disabled:cursor-not-allowed disabled:opacity-40",
                  config.tone === "danger"
                    ? "border-accent-red text-accent-red"
                    : "border-border text-text",
                )}
                disabled={disabled}
                onClick={() => mutation.mutate({ action: config.action, draft })}
              >
                {config.label}
              </button>
            </div>
          </div>
        );
      })}
      {mutation.isError && (
        <div className="py-3 text-[12px] text-accent-red">
          {mutation.error instanceof Error ? mutation.error.message : "Control request failed"}
        </div>
      )}
      {mutation.isSuccess && (
        <div className="py-3 text-[12px] text-accent-green">
          Command #{mutation.data.command_id} queued as {mutation.data.status}.
        </div>
      )}
    </div>
  );
}

function renderAuditRow(entry: LiveControlAuditRow) {
  return (
    <ActivityRow
      key={entry.id}
      title={entry.action}
      subtitle={entry.target ?? "control audit"}
      right={formatDateTime(entry.timestamp_ms)}
    />
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

  const shadow = summary.shadow_summary;
  const account = summary.real_account_summary;
  const latestSession = sessionsQuery.data?.sessions[0] ?? null;
  const isAdmin = user?.role === "admin";
  const hasAccountSnapshot = account.latest_snapshot_at_ms != null;
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

  if (isPaper) {
    return (
      <div className="space-y-3">
        <Surface className="p-3">
          <div className="text-[15px] font-semibold tracking-tight">Paper mode</div>
          <p className="mt-1 text-[12px] text-muted">
            Simulated trades only. Polymarket account and venue activity are inactive. Current market, open trades, and recent executions live on Overview and Trades.
          </p>
          <div className="mt-4 space-y-0">
            <ValueLine
              label="Process"
              value={
                <StatusChip
                  label={processStateLabel(summary.process_state)}
                  tone={processStateTone(summary.process_state)}
                  compact
                />
              }
            />
            <ValueLine label="Mode" value={runtimeModeLabel(summary.runtime_mode)} />
            <ValueLine label="Open shadow trades" value={shadow.open_trades.toString()} />
            <ValueLine
              label="Last tick"
              value={shadow.last_tick_at != null ? formatDateTime(shadow.last_tick_at) : "n/a"}
            />
          </div>
        </Surface>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {detailPanelsDegraded && (
        <Surface className="border-accent-blue/50 p-3">
          <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
            <StatusChip label="Detail panels degraded" tone="warning" compact />
            <div className="text-[12px] text-muted">
              Summary is current, but some execution detail panels are unavailable:{" "}
              {detailQueryFailures.join(", ")}.
            </div>
          </div>
        </Surface>
      )}

      {summary.alerts.length > 0 && (
        <SectionCard title="Alerts">
          <AlertList alerts={summary.alerts} />
        </SectionCard>
      )}

      {summary.trading_state === "halted" && (
        <Surface className="border-accent-red/70 p-3">
          <StatusChip label="Session halted" tone="danger" compact />
          <p className="mt-2 text-[12px] text-muted">
            This session cannot be re-armed from the dashboard. Export the run, analyze the halt,
            and start a new session only after a separate operator decision.
          </p>
        </Surface>
      )}

      {summary.trading_state === "unknown_order" && (
        <Surface className="border-accent-red/70 p-3">
          <StatusChip label="Unknown order state" tone="danger" compact />
          <p className="mt-2 text-[12px] text-muted">
            New submissions stay blocked until venue orders, fills, and account state are
            reconciled.
          </p>
        </Surface>
      )}

      <Surface className="p-3">
        <div className="grid gap-4 xl:grid-cols-[1.6fr_1fr]">
          <div>
            <div className="text-[15px] font-semibold tracking-tight">Runtime status</div>
            <p className="mt-1 text-[12px] text-muted">
              Venue readiness, account freshness, and reconciliation.
            </p>
            <div className="mt-4 space-y-0">
              <ValueLine
                label="Process"
                value={
                  <StatusChip
                    label={processStateLabel(summary.process_state)}
                    tone={processStateTone(summary.process_state)}
                    compact
                  />
                }
              />
              <ValueLine label="Mode" value={runtimeModeLabel(summary.runtime_mode)} />
              <ValueLine
                label="Trading state"
                value={
                  <StatusChip
                    label={tradingStateLabel(summary.trading_state)}
                    tone={tradingStateTone(summary.trading_state)}
                    compact
                    help={summary.trading_state === "readonly" ? help.readonly : undefined}
                  />
                }
              />
              <ValueLine
                label="Venue"
                value={
                  <StatusChip
                    label={summary.venue_health.label}
                    tone={healthTone(summary.venue_health)}
                    compact
                  />
                }
                detail={summary.venue_health.detail}
              />
              <ValueLine
                label="Account"
                value={
                  <StatusChip
                    label={summary.account_health.label}
                    tone={healthTone(summary.account_health)}
                    compact
                  />
                }
                detail={summary.account_health.detail}
              />
              <ValueLine
                label="Reconciliation"
                value={
                  <StatusChip
                    label={summary.reconciliation_health.label}
                    tone={healthTone(summary.reconciliation_health)}
                    compact
                  />
                }
                detail={summary.reconciliation_health.detail}
              />
            </div>
          </div>

          <div>
            <div className="mb-2 text-[13px] font-semibold tracking-tight">
              Polymarket account
            </div>
            {hasAccountSnapshot ? (
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
                    value:
                      account.total_equity != null ? formatUsd(account.total_equity) : "n/a",
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
                  { label: "Open orders", value: account.open_orders.toString() },
                  {
                    label: "User stream",
                    value: userStreamSummary(account),
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
            ) : (
              <StateEmpty message={empty.noPolymarketSnapshot} />
            )}
          </div>
        </div>
      </Surface>

      <SectionCard title="Controls">
        <ControlCommandPanel
          botId={botId}
          summary={summary}
          latestSession={latestSession}
          isAdmin={isAdmin}
        />
      </SectionCard>

      <SectionCard title="Venue activity">
        <div className="grid gap-4 xl:grid-cols-2">
          <ActivityPanel
            title="Control audit"
            loading={controlAuditQuery.isLoading}
            error={controlAuditQuery.isError ? controlAuditQuery.error : null}
            loadingMessage="Loading control audit..."
            errorMessage="Control audit is currently unavailable"
            emptyMessage="No control commands recorded."
            count={controlAuditQuery.data?.entries.length ?? 0}
          >
            {controlAuditQuery.data?.entries.map(renderAuditRow)}
          </ActivityPanel>
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
      </SectionCard>

      <details className="border border-border bg-bg">
        <summary className="cursor-pointer px-3 py-2 text-[13px] font-semibold tracking-tight">
          Session and wallet
        </summary>
        <div className="border-t border-border p-3">
          {sessionsQuery.isError && (
            <div className="mb-3 text-[12px] text-accent-blue">
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
              { label: "Provider", value: account.provider ?? "n/a" },
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
                label: "Latest session row",
                value:
                  latestSession != null
                    ? `${latestSession.execution_mode} / ${latestSession.status}`
                    : "n/a",
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
