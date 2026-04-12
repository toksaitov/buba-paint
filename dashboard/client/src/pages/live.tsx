import { useOutletContext } from "react-router-dom";
import { Loading } from "../components/common/loading";
import {
  useLiveFills,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
  useLiveStatus,
} from "../hooks/use-live-status";
import { formatDateTime, formatUsd } from "../lib/utils";

function parseStrategies(enabledStrategiesJson: string | null | undefined): string[] {
  if (!enabledStrategiesJson) return [];
  try {
    const parsed = JSON.parse(enabledStrategiesJson);
    return Array.isArray(parsed)
      ? parsed.filter((value): value is string => typeof value === "string")
      : [];
  } catch {
    return [];
  }
}

function parseDetails(detailsJson: string | null | undefined): Record<string, unknown> | null {
  if (!detailsJson) return null;
  try {
    const parsed = JSON.parse(detailsJson);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function detailString(
  details: Record<string, unknown> | null,
  key: string,
): string | null {
  const value = details?.[key];
  return typeof value === "string" ? value : null;
}

function detailNumber(
  details: Record<string, unknown> | null,
  key: string,
): number | null {
  const value = details?.[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function LivePage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data: status, isLoading } = useLiveStatus(botId);
  const { data: sessions } = useLiveSessions(botId, 10);
  const { data: orders } = useLiveOrders(botId, 10);
  const { data: fills } = useLiveFills(botId, 10);
  const { data: redemptions } = useLiveRedemptions(botId, 10);
  const { data: reconciliation } = useLiveReconciliation(botId, 10);

  if (isLoading || !status) return <Loading />;

  const latestSession = status.latest_session;
  const latestAccount = status.latest_account_snapshot;
  const enabledStrategies = parseStrategies(latestSession?.enabled_strategies_json);
  const sessionDetails = parseDetails(latestSession?.details_json);
  const accountDetails = parseDetails(latestAccount?.details_json);
  const provider =
    (typeof sessionDetails?.provider === "string" ? sessionDetails.provider : null) ??
    (typeof accountDetails?.provider === "string" ? accountDetails.provider : null);
  const userStreamStatus =
    detailString(accountDetails, "user_stream_status") ??
    detailString(sessionDetails, "user_stream_status");
  const lastUserStreamConnectedAtMs =
    detailNumber(accountDetails, "last_user_stream_connected_at_ms") ??
    detailNumber(sessionDetails, "last_user_stream_connected_at_ms");
  const lastUserStreamEventAtMs =
    detailNumber(accountDetails, "last_user_stream_event_at_ms") ??
    detailNumber(sessionDetails, "last_user_stream_event_at_ms");
  const lastAccountRefreshAtMs =
    detailNumber(accountDetails, "last_successful_account_refresh_at_ms") ??
    detailNumber(sessionDetails, "last_successful_account_refresh_at_ms");

  return (
    <div className="space-y-3">
      <section className="border border-border bg-bg">
        <div className="border-b border-border px-3 py-2">
          <h1 className="text-sm font-semibold">Live Readiness</h1>
          <p className="text-xs text-muted">
            Live trading remains explicitly gated. This page surfaces live sessions,
            account-state decomposition, venue activity, and reconciliation health.
          </p>
          <p className="mt-2 text-xs text-muted">
            <code>live_readonly</code> now runs against the real authenticated venue boundary.
            <code> live_trading</code> remains explicitly gated and cannot place orders yet.
          </p>
          <p className="mt-2 text-xs text-muted">
            The old dashboard pages remain the shadow paper view for strategy performance. This
            page is the real venue/account and reconciliation view.
          </p>
          {provider === "stub" && (
            <p className="mt-2 text-xs text-muted">
              Provider status: stub. Venue geoblock, allowance, user-stream, and remote cash
              checks are not verified by this branch yet.
            </p>
          )}
          {!latestSession && (
            <p className="mt-2 text-xs text-muted">
              No live_readonly session has been recorded yet. Start <code>buba-paint live</code>{" "}
              with <code>EXECUTION_MODE=live_readonly</code> to verify the authenticated venue
              boundary.
            </p>
          )}
        </div>
        <div className="grid grid-cols-2 lg:grid-cols-5 gap-px bg-border">
          <div className="bg-bg px-3 py-3">
            <div className="text-[11px] uppercase tracking-widest text-muted">Mode</div>
            <div className="mt-1 text-sm font-semibold">
              {latestSession?.execution_mode ?? "paper"}
            </div>
          </div>
          <div className="bg-bg px-3 py-3">
            <div className="text-[11px] uppercase tracking-widest text-muted">Cash Available</div>
            <div className="mt-1 text-sm font-semibold">
              {latestAccount ? formatUsd(latestAccount.cash_available) : "n/a"}
            </div>
          </div>
          <div className="bg-bg px-3 py-3">
            <div className="text-[11px] uppercase tracking-widest text-muted">Reserved</div>
            <div className="mt-1 text-sm font-semibold">
              {latestAccount ? formatUsd(latestAccount.cash_reserved_for_orders) : "n/a"}
            </div>
          </div>
          <div className="bg-bg px-3 py-3">
            <div className="text-[11px] uppercase tracking-widest text-muted">Open Orders</div>
            <div className="mt-1 text-sm font-semibold">{status.open_orders}</div>
          </div>
          <div className="bg-bg px-3 py-3">
            <div className="text-[11px] uppercase tracking-widest text-muted">Critical Recon</div>
            <div className="mt-1 text-sm font-semibold">
              {status.critical_reconciliation_events}
            </div>
          </div>
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-px bg-border border-t border-border">
          <div className="bg-bg px-3 py-3 text-xs">
            <div className="font-semibold text-text">Session</div>
            {latestSession ? (
              <div className="mt-2 space-y-1 text-muted">
                <div>ID {latestSession.id}</div>
                <div>Status {latestSession.status}</div>
                <div>Started {formatDateTime(latestSession.started_at_ms)}</div>
                <div>Wallet {latestSession.wallet_address ?? "n/a"}</div>
                <div>Proxy {latestSession.proxy_wallet ?? "n/a"}</div>
                <div>Cash cap {formatUsd(latestSession.cash_cap_usd)}</div>
                <div>Fingerprint {latestSession.config_fingerprint}</div>
                <div>
                  Strategies {enabledStrategies.length > 0 ? enabledStrategies.join(", ") : "n/a"}
                </div>
                {provider && <div>Provider {provider}</div>}
                <div>User stream {userStreamStatus ?? "n/a"}</div>
                <div>
                  Last user stream connect{" "}
                  {lastUserStreamConnectedAtMs != null
                    ? formatDateTime(lastUserStreamConnectedAtMs)
                    : "n/a"}
                </div>
                <div>
                  Last user stream event{" "}
                  {lastUserStreamEventAtMs != null
                    ? formatDateTime(lastUserStreamEventAtMs)
                    : "n/a"}
                </div>
              </div>
            ) : (
              <div className="mt-2 text-muted">
                No live session has been recorded yet.
              </div>
            )}
          </div>
          <div className="bg-bg px-3 py-3 text-xs">
            <div className="font-semibold text-text">Account Decomposition</div>
            {latestAccount ? (
              <div className="mt-2 space-y-1 text-muted">
                <div>Snapshot {formatDateTime(latestAccount.timestamp_ms)}</div>
                <div>Cash {formatUsd(latestAccount.cash_available)}</div>
                <div>Inventory {formatUsd(latestAccount.inventory_mark_value)}</div>
                <div>Redeemable {formatUsd(latestAccount.redeemable_value)}</div>
                <div>Pending redeem {formatUsd(latestAccount.pending_redeem_value)}</div>
                <div>Total equity {formatUsd(latestAccount.total_equity)}</div>
                <div>
                  Allowance{" "}
                  {latestAccount.allowance_available != null
                    ? formatUsd(latestAccount.allowance_available)
                    : "n/a"}
                </div>
                <div>
                  Last account refresh{" "}
                  {lastAccountRefreshAtMs != null
                    ? formatDateTime(lastAccountRefreshAtMs)
                    : "n/a"}
                </div>
              </div>
            ) : (
              <div className="mt-2 text-muted">
                No live account snapshots have been recorded yet.
              </div>
            )}
          </div>
        </div>
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-2 gap-3">
        <div className="border border-border bg-bg">
          <div className="border-b border-border px-3 py-2 text-sm font-semibold">
            Recent Sessions
          </div>
          <div className="divide-y divide-border text-xs">
            {(sessions?.sessions ?? []).map((session) => (
              <div key={session.id} className="px-3 py-2">
                <div className="font-medium">
                  #{session.id} {session.execution_mode} {session.status}
                </div>
                <div className="text-muted">
                  {formatDateTime(session.started_at_ms)} · cap {formatUsd(session.cash_cap_usd)}
                </div>
              </div>
            ))}
            {(sessions?.sessions ?? []).length === 0 && (
              <div className="px-3 py-3 text-muted">No live sessions recorded.</div>
            )}
          </div>
        </div>

        <div className="border border-border bg-bg">
          <div className="border-b border-border px-3 py-2 text-sm font-semibold">
            Reconciliation
          </div>
          <div className="divide-y divide-border text-xs">
            {(reconciliation?.events ?? []).map((event) => (
              <div key={event.id} className="px-3 py-2">
                <div className="font-medium">
                  {event.severity} · {event.event_type}
                </div>
                <div className="text-muted">{formatDateTime(event.timestamp_ms)}</div>
              </div>
            ))}
            {(reconciliation?.events ?? []).length === 0 && (
              <div className="px-3 py-3 text-muted">No reconciliation events recorded.</div>
            )}
          </div>
        </div>
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-3 gap-3">
        <div className="border border-border bg-bg">
          <div className="border-b border-border px-3 py-2 text-sm font-semibold">
            Recent Orders
          </div>
          <div className="divide-y divide-border text-xs">
            {(orders?.orders ?? []).map((order) => (
              <div key={order.id} className="px-3 py-2">
                <div className="font-medium">
                  {order.status} · {order.side} · {order.order_type}
                </div>
                <div className="text-muted">
                  {order.market_id} · {formatDateTime(order.updated_at_ms)}
                </div>
              </div>
            ))}
            {(orders?.orders ?? []).length === 0 && (
              <div className="px-3 py-3 text-muted">No live orders recorded.</div>
            )}
          </div>
        </div>

        <div className="border border-border bg-bg">
          <div className="border-b border-border px-3 py-2 text-sm font-semibold">
            Recent Fills
          </div>
          <div className="divide-y divide-border text-xs">
            {(fills?.fills ?? []).map((fill) => (
              <div key={fill.id} className="px-3 py-2">
                <div className="font-medium">
                  {fill.status} · {fill.size.toFixed(2)} @ {fill.price.toFixed(3)}
                </div>
                <div className="text-muted">{formatDateTime(fill.filled_at_ms)}</div>
              </div>
            ))}
            {(fills?.fills ?? []).length === 0 && (
              <div className="px-3 py-3 text-muted">No live fills recorded.</div>
            )}
          </div>
        </div>

        <div className="border border-border bg-bg">
          <div className="border-b border-border px-3 py-2 text-sm font-semibold">
            Redemptions
          </div>
          <div className="divide-y divide-border text-xs">
            {(redemptions?.redemptions ?? []).map((redemption) => (
              <div key={redemption.id} className="px-3 py-2">
                <div className="font-medium">
                  {redemption.status} · {formatUsd(redemption.redeemable_value)}
                </div>
                <div className="text-muted">
                  {redemption.market_id} · {formatDateTime(redemption.detected_redeemable_at_ms)}
                </div>
              </div>
            ))}
            {(redemptions?.redemptions ?? []).length === 0 && (
              <div className="px-3 py-3 text-muted">No live redemptions recorded.</div>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
