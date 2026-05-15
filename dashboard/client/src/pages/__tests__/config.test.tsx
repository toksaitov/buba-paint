import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-runtime-config", () => ({
  useRuntimeConfig: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { useRuntimeConfig } from "../../hooks/use-runtime-config";
import { ConfigPage } from "../config";
import type { RuntimeConfigSnapshot } from "../../lib/types";

const mockUseRuntimeConfig = vi.mocked(useRuntimeConfig);

function baseSnapshot(overrides: Partial<RuntimeConfigSnapshot> = {}): RuntimeConfigSnapshot {
  return {
    execution_mode: "live_readonly",
    runtime_name: "probe-2026-05",
    process_start_time_ms: 1_700_000_000_000,
    db_path_label: "/runtime/paint.db",
    config_fingerprint: "fingerprint-canary-1",
    git_sha: "abc123",
    package_version: "0.6.0",
    feed_event_storage_profile: "replay_grade",
    live_runtime_max_db_bytes: 1_073_741_824,
    latency_arb: {
      enabled: true,
      momentum_threshold: 0.0008,
      min_ask: 0.3,
      max_ask: 0.6,
      cooldown_ms: 60_000,
      adaptive_window_ms: 5_000,
      max_position_fraction: 0.125,
    },
    spread_capture: {
      enabled: false,
      threshold: 0.5,
      min_ask: 0.1,
      max_leg_skew_ms: 250,
      max_quote_churn_per_s: 6,
      max_position_fraction: null,
    },
    calm_persistence: {
      enabled: false,
      min_window_time_ms: 60_000,
      max_window_time_ms: 240_000,
      max_ask: 0.6,
      min_abs_distance_bps: 5,
      distance_vol_ratio_threshold: 0.5,
      max_realized_vol_15s_bps: 10,
      max_open_crosses_30s: 6,
      max_quote_churn_per_s: 4,
      min_alignment_fraction: 0.7,
      max_fair_bias: 0.05,
      min_expected_edge: 0.005,
      max_position_fraction: null,
    },
    risk: {
      starting_balance: 100,
      max_position_fraction: 0.25,
      min_bet_usd: 1,
      max_drawdown_pct: 0.5,
      live_session_cash_cap_usd: 100,
      live_max_single_order_usd: 10,
      live_max_open_notional_usd: 50,
      live_max_daily_loss_usd: 25,
      live_max_session_drawdown_usd: 30,
      live_min_required_cash_usd: 1,
    },
    pending_settlement: {
      family_reserve_fraction: 1.0,
      global_reserve_fraction: 1.0,
      counts_as_open_position: true,
    },
    fees: {
      taker_fee_rate: 0.001,
      taker_fee_exponent: 1,
      sim_order_latency_ms: 250,
    },
    feed_freshness: {
      max_book_staleness_ms: 250,
      max_signal_feed_age_ms: 1_000,
      max_quote_age_ms: 500,
      clob_no_message_reconnect_ms: 60_000,
      binance_no_message_reconnect_ms: 30_000,
    },
    worker_budgets: {
      feed_event_writer_queue_capacity: 4_096,
      feed_event_writer_batch_size: 256,
      feed_event_writer_flush_ms: 50,
      feed_event_writer_max_lag_ms: 5_000,
      clob_replay_block_max_rows: 10_000,
      clob_replay_block_max_ms: 1_000,
      clob_replay_block_zstd_level: 3,
      live_decision_queue_capacity: 256,
      live_decision_output_queue_capacity: 256,
      live_runtime_persistence_queue_capacity: 1_024,
      live_submission_queue_capacity: 128,
      max_live_decision_age_ms: 1_500,
      worker_shutdown_timeout_ms: 5_000,
    },
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: true,
    isError: false,
    data: undefined,
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("shows empty-state when snapshot is null", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: { snapshot: null, snapshot_recorded_at_ms: null, uptime_secs: null },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);
  expect(
    screen.getByText(/No parameter snapshot has been written yet/i),
  ).toBeInTheDocument();
});

test("renders all eleven section headings when snapshot is present", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot(),
      snapshot_recorded_at_ms: 1_700_000_000_000,
      uptime_secs: 120,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  for (const heading of [
    "Identity",
    "Storage and replay",
    "Strategies",
    "Latency-arb params",
    "Spread-capture params",
    "Calm-persistence params",
    "Risk and bankroll",
    "Pending settlement",
    "Fees and paper assumptions",
    "Feed freshness",
    "Worker budgets",
  ]) {
    expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
  }
});

test("marks each strategy enabled or disabled with a status chip", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot(),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  const enabledChips = screen.getAllByText("Enabled");
  const disabledChips = screen.getAllByText("Disabled");
  expect(enabledChips.length).toBeGreaterThanOrEqual(1);
  expect(disabledChips.length).toBeGreaterThanOrEqual(2);
});

test("exposes the config fingerprint via a CopyButton", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot(),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  const shortDisplay = screen.getByTitle("fingerprint-canary-1");
  expect(shortDisplay).toBeInTheDocument();
  expect(shortDisplay.textContent).toMatch(/^fingerprint-.{0,4}…?$/);
  expect(
    screen.getByRole("button", { name: /copy config fingerprint/i }),
  ).toBeInTheDocument();
});

test("truncates a very long config fingerprint to a short prefix", () => {
  const longFingerprint = '"cash_cap_usd":100.0,"enabled_strategies":["latency-arb"],"execution_mode":"live_readonly"';
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot({ config_fingerprint: longFingerprint }),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  expect(screen.queryByText(longFingerprint)).not.toBeInTheDocument();
  const truncated = screen.getByTitle(longFingerprint);
  expect(truncated.textContent?.length).toBeLessThanOrEqual(13);
  expect(truncated.textContent).toMatch(/…$/);
});

test("shows the Run 012 canary chip when latency-arb params match the canary profile", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot(),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  expect(screen.getByText(/Run 012 latency-only profile/)).toBeInTheDocument();
});

test("does not show the canary chip when params differ", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot({
        latency_arb: {
          enabled: true,
          momentum_threshold: 0.001,
          min_ask: 0.3,
          max_ask: 0.6,
          cooldown_ms: 60_000,
          adaptive_window_ms: 5_000,
          max_position_fraction: 0.125,
        },
      }),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  render(<ConfigPage />);

  expect(screen.queryByText(/Run 012 latency-only profile/)).not.toBeInTheDocument();
});

test("does not render forbidden secret substrings anywhere on the page", () => {
  mockUseRuntimeConfig.mockReturnValue({
    isLoading: false,
    isError: false,
    data: {
      snapshot: baseSnapshot(),
      snapshot_recorded_at_ms: 1,
      uptime_secs: 10,
    },
    error: null,
  } as ReturnType<typeof useRuntimeConfig>);
  const { container } = render(<ConfigPage />);
  const text = container.textContent ?? "";
  for (const forbidden of ["AGENT_SECRET", "JWT_SECRET", "private_key", "relayer_secret", "password"]) {
    expect(text).not.toContain(forbidden);
  }
});
