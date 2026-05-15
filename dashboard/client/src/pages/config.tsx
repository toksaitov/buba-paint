import { useOutletContext } from "react-router-dom";
import {
  Banner,
  CopyButton,
  KeyValueList,
  RelativeTime,
  SectionCard,
  StateEmpty,
  StatusChip,
} from "../components/ui/dashboard-primitives";
import { Loading } from "../components/common/loading";
import { useRuntimeConfig } from "../hooks/use-runtime-config";
import type { RuntimeConfigSnapshot } from "../lib/types";

type ChipTone = "neutral" | "muted" | "success" | "warning" | "danger";

function modeTone(mode: string): ChipTone {
  if (mode === "live_trading") return "warning";
  if (mode === "live_readonly") return "success";
  return "muted";
}

function formatUptime(secs: number | null): string {
  if (secs == null) return "n/a";
  if (secs < 60) return `${secs}s`;
  const minutes = Math.floor(secs / 60);
  if (minutes < 60) return `${minutes}m ${secs % 60}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[index]}`;
}

function formatMs(ms: number): string {
  if (ms === 0) return "0";
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms % 1000 === 0 ? 0 : 2)} s`;
  return `${(ms / 60_000).toFixed(1)} min`;
}

function formatNumber(value: number, digits = 4): string {
  if (Number.isInteger(value)) return value.toString();
  return value.toFixed(digits);
}

function formatFraction(value: number | null): string {
  if (value == null) return "default";
  return formatNumber(value, 4);
}

function formatUsd(value: number): string {
  return `$${value.toFixed(2)}`;
}

function matchesRun012LatencyProfile(s: RuntimeConfigSnapshot): boolean {
  return (
    s.latency_arb.enabled &&
    !s.spread_capture.enabled &&
    !s.calm_persistence.enabled &&
    s.latency_arb.momentum_threshold === 0.0008 &&
    s.latency_arb.min_ask === 0.3 &&
    s.latency_arb.max_ask === 0.6 &&
    s.latency_arb.max_position_fraction === 0.125
  );
}

function FingerprintCell({ value }: { value: string }) {
  const short = value.length > 16 ? `${value.slice(0, 12)}…` : value;
  return (
    <span className="inline-flex max-w-full min-w-0 items-center justify-end gap-2">
      <span
        className="truncate font-mono text-[11px]"
        title={value}
      >
        {short}
      </span>
      <CopyButton value={value} size="sm" ariaLabel="Copy config fingerprint" />
    </span>
  );
}

export function ConfigPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const { data, isLoading, isError, error } = useRuntimeConfig(botId);

  if (isLoading) return <Loading label="Loading runtime config" />;
  if (isError) {
    return (
      <Banner tone="danger" title="Unable to load runtime config">
        {error instanceof Error ? error.message : "Try refreshing the page."}
      </Banner>
    );
  }
  if (!data?.snapshot) {
    return (
      <SectionCard title="Runtime parameters">
        <StateEmpty message="No parameter snapshot has been written yet. The bot writes the snapshot once at startup. This page will populate after the next bot restart." />
      </SectionCard>
    );
  }

  const s = data.snapshot;
  const isCanary = matchesRun012LatencyProfile(s);

  const identityItems = [
    { label: "Execution mode", value: <StatusChip label={s.execution_mode} tone={modeTone(s.execution_mode)} compact /> },
    { label: "Runtime name", value: s.runtime_name ?? "n/a" },
    { label: "Process started", value: <RelativeTime epochMs={s.process_start_time_ms} /> },
    { label: "Uptime", value: <span className="tabular-nums">{formatUptime(data.uptime_secs)}</span> },
    { label: "Snapshot recorded", value: <RelativeTime epochMs={data.snapshot_recorded_at_ms ?? undefined} /> },
    { label: "Git SHA", value: <span className="font-mono text-[11px]">{s.git_sha}</span> },
    { label: "Package version", value: <span className="font-mono text-[11px]">{s.package_version}</span> },
    { label: "DB path", value: <span className="font-mono text-[11px]">{s.db_path_label}</span> },
    { label: "Config fingerprint", value: <FingerprintCell value={s.config_fingerprint} /> },
    ...(isCanary
      ? [{
          label: "Canary",
          value: <StatusChip label="Run 012 latency-only profile" tone="success" compact />,
        }]
      : []),
  ];

  const storageItems = [
    { label: "Feed event storage profile", value: s.feed_event_storage_profile },
    { label: "Live runtime max DB size", value: formatBytes(s.live_runtime_max_db_bytes) },
  ];

  const strategiesItems = [
    { label: "Latency-arb", value: <StatusChip label={s.latency_arb.enabled ? "Enabled" : "Disabled"} tone={s.latency_arb.enabled ? "success" : "muted"} compact /> },
    { label: "Spread-capture", value: <StatusChip label={s.spread_capture.enabled ? "Enabled" : "Disabled"} tone={s.spread_capture.enabled ? "success" : "muted"} compact /> },
    { label: "Calm-persistence", value: <StatusChip label={s.calm_persistence.enabled ? "Enabled" : "Disabled"} tone={s.calm_persistence.enabled ? "success" : "muted"} compact /> },
  ];

  const latencyArbItems = [
    { label: "Momentum threshold", value: formatNumber(s.latency_arb.momentum_threshold, 5) },
    { label: "Min ask", value: formatNumber(s.latency_arb.min_ask, 3) },
    { label: "Max ask", value: formatNumber(s.latency_arb.max_ask, 3) },
    { label: "Cooldown", value: formatMs(s.latency_arb.cooldown_ms) },
    { label: "Adaptive window", value: formatMs(s.latency_arb.adaptive_window_ms) },
    { label: "Max position fraction", value: formatFraction(s.latency_arb.max_position_fraction) },
  ];

  const spreadItems = [
    { label: "Threshold", value: formatNumber(s.spread_capture.threshold, 4) },
    { label: "Min ask", value: formatNumber(s.spread_capture.min_ask, 3) },
    { label: "Max leg skew", value: formatMs(s.spread_capture.max_leg_skew_ms) },
    { label: "Max quote churn / s", value: formatNumber(s.spread_capture.max_quote_churn_per_s, 2) },
    { label: "Max position fraction", value: formatFraction(s.spread_capture.max_position_fraction) },
  ];

  const calmItems = [
    { label: "Min window time", value: formatMs(s.calm_persistence.min_window_time_ms) },
    { label: "Max window time", value: formatMs(s.calm_persistence.max_window_time_ms) },
    { label: "Max ask", value: formatNumber(s.calm_persistence.max_ask, 3) },
    { label: "Min abs distance (bps)", value: formatNumber(s.calm_persistence.min_abs_distance_bps, 2) },
    { label: "Distance/vol ratio threshold", value: formatNumber(s.calm_persistence.distance_vol_ratio_threshold, 3) },
    { label: "Max realized vol 15s (bps)", value: formatNumber(s.calm_persistence.max_realized_vol_15s_bps, 2) },
    { label: "Max open crosses (30s)", value: s.calm_persistence.max_open_crosses_30s.toString() },
    { label: "Max quote churn / s", value: formatNumber(s.calm_persistence.max_quote_churn_per_s, 2) },
    { label: "Min alignment fraction", value: formatNumber(s.calm_persistence.min_alignment_fraction, 3) },
    { label: "Max fair bias", value: formatNumber(s.calm_persistence.max_fair_bias, 4) },
    { label: "Min expected edge", value: formatNumber(s.calm_persistence.min_expected_edge, 4) },
    { label: "Max position fraction", value: formatFraction(s.calm_persistence.max_position_fraction) },
  ];

  const riskItems = [
    { label: "Starting balance", value: formatUsd(s.risk.starting_balance) },
    { label: "Max position fraction", value: formatNumber(s.risk.max_position_fraction, 4) },
    { label: "Min bet", value: formatUsd(s.risk.min_bet_usd) },
    { label: "Max drawdown %", value: formatNumber(s.risk.max_drawdown_pct, 4) },
    { label: "Live session cash cap", value: formatUsd(s.risk.live_session_cash_cap_usd), tone: "warning" as ChipTone },
    { label: "Live max single order", value: formatUsd(s.risk.live_max_single_order_usd), tone: "warning" as ChipTone },
    { label: "Live max open notional", value: formatUsd(s.risk.live_max_open_notional_usd), tone: "warning" as ChipTone },
    { label: "Live max daily loss", value: formatUsd(s.risk.live_max_daily_loss_usd), tone: "warning" as ChipTone },
    { label: "Live max session drawdown", value: formatUsd(s.risk.live_max_session_drawdown_usd), tone: "warning" as ChipTone },
    { label: "Live min required cash", value: formatUsd(s.risk.live_min_required_cash_usd) },
  ];

  const pendingItems = [
    { label: "Family reserve fraction", value: formatNumber(s.pending_settlement.family_reserve_fraction, 4) },
    { label: "Global reserve fraction", value: formatNumber(s.pending_settlement.global_reserve_fraction, 4) },
    { label: "Counts as open position", value: s.pending_settlement.counts_as_open_position ? "Yes" : "No" },
  ];

  const feeItems = [
    { label: "Taker fee rate", value: formatNumber(s.fees.taker_fee_rate, 5) },
    { label: "Taker fee exponent", value: s.fees.taker_fee_exponent.toString() },
    { label: "Sim order latency", value: formatMs(s.fees.sim_order_latency_ms) },
  ];

  const feedItems = [
    { label: "Max book staleness", value: formatMs(s.feed_freshness.max_book_staleness_ms) },
    { label: "Max signal feed age", value: formatMs(s.feed_freshness.max_signal_feed_age_ms) },
    { label: "Max quote age", value: formatMs(s.feed_freshness.max_quote_age_ms) },
    { label: "CLOB no-msg reconnect", value: formatMs(s.feed_freshness.clob_no_message_reconnect_ms) },
    { label: "Binance no-msg reconnect", value: formatMs(s.feed_freshness.binance_no_message_reconnect_ms) },
  ];

  const workerItems = [
    { label: "Feed writer queue capacity", value: s.worker_budgets.feed_event_writer_queue_capacity.toString() },
    { label: "Feed writer batch size", value: s.worker_budgets.feed_event_writer_batch_size.toString() },
    { label: "Feed writer flush", value: formatMs(s.worker_budgets.feed_event_writer_flush_ms) },
    { label: "Feed writer max lag", value: formatMs(s.worker_budgets.feed_event_writer_max_lag_ms) },
    { label: "CLOB block max rows", value: s.worker_budgets.clob_replay_block_max_rows.toString() },
    { label: "CLOB block max age", value: formatMs(s.worker_budgets.clob_replay_block_max_ms) },
    { label: "CLOB block zstd level", value: s.worker_budgets.clob_replay_block_zstd_level.toString() },
    { label: "Decision queue capacity", value: s.worker_budgets.live_decision_queue_capacity.toString() },
    { label: "Decision output capacity", value: s.worker_budgets.live_decision_output_queue_capacity.toString() },
    { label: "Persistence queue capacity", value: s.worker_budgets.live_runtime_persistence_queue_capacity.toString() },
    { label: "Submission queue capacity", value: s.worker_budgets.live_submission_queue_capacity.toString() },
    { label: "Max decision age", value: formatMs(s.worker_budgets.max_live_decision_age_ms) },
    { label: "Worker shutdown timeout", value: formatMs(s.worker_budgets.worker_shutdown_timeout_ms) },
  ];

  return (
    <div className="space-y-3">
      <SectionCard title="Identity">
        <KeyValueList items={identityItems} />
      </SectionCard>

      <SectionCard title="Storage and replay">
        <KeyValueList items={storageItems} />
      </SectionCard>

      <SectionCard title="Strategies">
        <KeyValueList items={strategiesItems} />
      </SectionCard>

      <SectionCard title="Latency-arb params">
        <KeyValueList items={latencyArbItems} columns={2} />
      </SectionCard>

      <SectionCard title="Spread-capture params">
        <KeyValueList items={spreadItems} columns={2} />
      </SectionCard>

      <SectionCard title="Calm-persistence params">
        <KeyValueList items={calmItems} columns={2} />
      </SectionCard>

      <SectionCard title="Risk and bankroll">
        <KeyValueList items={riskItems} columns={2} />
      </SectionCard>

      <SectionCard title="Pending settlement">
        <KeyValueList items={pendingItems} />
      </SectionCard>

      <SectionCard title="Fees and paper assumptions">
        <KeyValueList items={feeItems} />
      </SectionCard>

      <SectionCard title="Feed freshness">
        <KeyValueList items={feedItems} columns={2} />
      </SectionCard>

      <SectionCard title="Worker budgets">
        <KeyValueList items={workerItems} columns={2} />
      </SectionCard>
    </div>
  );
}
