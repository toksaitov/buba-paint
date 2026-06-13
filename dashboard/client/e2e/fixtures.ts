import type { Page, Route } from "@playwright/test";

function json(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: "application/json",
    body: JSON.stringify(body),
  });
}

export async function installMockWebSocket(page: Page) {
  await page.addInitScript(() => {
    class MockWebSocket {
      static CONNECTING = 0;
      static OPEN = 1;
      static CLOSING = 2;
      static CLOSED = 3;

      readyState = MockWebSocket.CONNECTING;
      url: string;
      onopen: ((event: Event) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;

      constructor(url: string) {
        this.url = url;
        queueMicrotask(() => {
          this.readyState = MockWebSocket.OPEN;
          this.onopen?.(new Event("open"));
        });
      }

      send() {}

      close() {
        this.readyState = MockWebSocket.CLOSED;
        this.onclose?.(new CloseEvent("close"));
      }

      addEventListener(type: string, listener: EventListener) {
        if (type === "open") this.onopen = listener as (event: Event) => void;
        if (type === "close") this.onclose = listener as (event: CloseEvent) => void;
        if (type === "message") this.onmessage = listener as (event: MessageEvent) => void;
        if (type === "error") this.onerror = listener as (event: Event) => void;
      }

      removeEventListener() {}
    }

    Object.defineProperty(window, "WebSocket", {
      value: MockWebSocket,
      writable: true,
    });
  });
}

export async function stubApi(page: Page) {
  await page.route("**/api/auth/login", async (route) => {
    await json(route, {
      token: "jwt-token",
      user: { id: "user-1", username: "admin", role: "admin" },
    });
  });

  await page.route("**/api/auth/me", async (route) => {
    await json(route, { id: "user-1", username: "admin", role: "admin" });
  });

  await page.route("**/api/bots", async (route) => {
    await json(route, {
      bots: [
        { id: "paint", name: "Paint" },
        { id: "paper-2", name: "Paper Two" },
      ],
    });
  });

  await page.route("**/api/bots/*/status", async (route) => {
    await json(route, {
      balance: 250.5,
      starting_balance: 200,
      execution_mode: "live_readonly",
      live_session_status: "readonly_ready",
      total_trades: 12,
      wins: 8,
      losses: 4,
      win_rate: 66.7,
      total_pnl: 50.5,
      max_drawdown_pct: 0.12,
      high_water_mark: 275.0,
      uptime_hours: 4.5,
      open_trades: 2,
      last_tick_at: 1_716_000_000_000,
      current_window: {
        market_id: "mkt-1",
        question: "Will BTC go up?",
        end_time: 1_716_000_300_000,
      },
    });
  });

  await page.route("**/api/bots/*/trading/summary", async (route) => {
    await json(route, {
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      process_state: "running",
      venue_health: {
        state: "healthy",
        label: "Venue connected",
        detail: "Authenticated venue monitoring and user-stream health are green.",
      },
      account_health: {
        state: "healthy",
        label: "Account tracked",
        detail: "Real cash, allowance, and equity decomposition are available.",
      },
      reconciliation_health: {
        state: "healthy",
        label: "Clean",
        detail: "No critical reconciliation events are currently recorded.",
      },
      shadow_summary: {
        balance: 250.5,
        starting_balance: 200,
        total_pnl: 50.5,
        total_trades: 12,
        wins: 8,
        losses: 4,
        win_rate: 0.667,
        max_drawdown_pct: 0.12,
        high_water_mark: 275,
        uptime_hours: 4.5,
        open_trades: 2,
        current_window: {
          market_id: "mkt-1",
          question: "Will BTC go up?",
          end_time: 1_716_000_300_000,
        },
      },
      real_account_summary: {
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: 1_716_000_000_000,
        cash_cap_usd: 100,
        available_cash: 99.17,
        reserved_cash: 0,
        inventory_mark_value: 0,
        redeemable_value: 0,
        pending_redeem_value: 0,
        total_equity: 99.17,
        allowance_available: 99.17,
        latest_snapshot_at_ms: 1_716_000_100_000,
        provider: "polymarket",
        user_stream_status: "ok",
        last_user_stream_connected_at_ms: 1_716_000_050_000,
        last_user_stream_event_at_ms: null,
        last_account_refresh_at_ms: 1_716_000_100_000,
        wallet_address: "0x1234",
        proxy_wallet: "0xabcd",
        enabled_strategies: ["latency-arb", "spread-capture"],
        open_orders: 0,
        pending_redemptions: 0,
        critical_reconciliation_events: 0,
        config_fingerprint: "{\"execution_mode\":\"live_readonly\"}",
      },
      capabilities: {
        preflight: { enabled: false, reason: "Dashboard preflight action is not wired in this pass." },
        arm: { enabled: false, reason: "Live trading remains gated in this pass." },
        disarm: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        cancel_all: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        stop_after_flat: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        redeem: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        kill_switch: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
      },
      alerts: [],
    });
  });

  await page.route("**/api/bots/*/trades**", async (route) => {
    await json(route, {
      trades: [
        {
          id: 1,
          timestamp: 1_716_000_000_000,
          market_id: "mkt-1",
          strategy: "latency-arb",
          side: "UP",
          token_id: "tok-up",
          entry_price: 0.54,
          size: 25,
          status: "closed",
          pnl: 12.5,
          settlement_price: 1,
          resolved_at: 1_716_000_100_000,
          fill_status: "filled",
          execution_fidelity: "legacy_snapshot",
          filled_size: 25,
          avg_fill_price: 0.54,
        },
      ],
      total: 1,
      page: 1,
      per_page: 50,
    });
  });

  await page.route("**/api/bots/*/balance**", async (route) => {
    await json(route, {
      entries: [
        { id: 1, timestamp: 1_716_000_000_000, event: "init", balance: 200 },
        { id: 2, timestamp: 1_716_000_100_000, event: "settlement", balance: 250.5 },
      ],
    });
  });

  await page.route("**/api/bots/*/equity/series**", async (route) => {
    await json(route, {
      baseline: { id: 1, timestamp: 0, event: "baseline", balance: 200 },
      points: [
        { id: 2, timestamp: 1_716_000_000_000, event: "init", balance: 200 },
        { id: 3, timestamp: 1_716_000_100_000, event: "settlement", balance: 250.5 },
      ],
    });
  });

  await page.route("**/api/bots/*/signals/groups**", async (route) => {
    await json(route, {
      groups: [
        {
          id: "mkt-1:latency-arb:UP:1716000000000",
          first_timestamp: 1_716_000_000_000,
          last_timestamp: 1_716_000_000_000,
          count: 1,
          strategy: "latency-arb",
          direction: "UP",
          market_id: "mkt-1",
          binance_price: 68000,
          chainlink_price: 68010,
          up_ask: 0.54,
          down_ask: 0.46,
          momentum: 0.0012,
        },
      ],
      raw_rows_scanned: 1,
      quiet_gap_ms: 5000,
    });
  });

  await page.route("**/api/bots/*/signals**", async (route) => {
    if (route.request().url().includes("/signals/groups")) {
      await json(route, {
        groups: [
          {
            id: "mkt-1:latency-arb:UP:1716000000000",
            first_timestamp: 1_716_000_000_000,
            last_timestamp: 1_716_000_000_000,
            count: 1,
            strategy: "latency-arb",
            direction: "UP",
            market_id: "mkt-1",
            binance_price: 68000,
            chainlink_price: 68010,
            up_ask: 0.54,
            down_ask: 0.46,
            momentum: 0.0012,
          },
        ],
        raw_rows_scanned: 1,
        quiet_gap_ms: 5000,
      });
      return;
    }
    await json(route, {
      signals: [
        {
          id: 10,
          timestamp: 1_716_000_000_000,
          strategy: "latency-arb",
          direction: "UP",
          binance_price: 68000,
          chainlink_price: 68010,
          up_ask: 0.54,
          down_ask: 0.46,
          metadata: "{\"momentum\":0.0012}",
          market_id: "mkt-1",
          execution_fidelity: "legacy_snapshot",
        },
      ],
    });
  });

  await page.route("**/api/bots/*/logs**", async (route) => {
    await json(route, {
      lines: ["booted paint", "connected to feeds", "latency arb signal"],
    });
  });

  await page.route("**/api/bots/*/live/status", async (route) => {
    await json(route, {
      latest_session: null,
      latest_account_snapshot: null,
      open_orders: 0,
      pending_redemptions: 0,
      critical_reconciliation_events: 0,
    });
  });

  await page.route("**/api/bots/*/live/sessions**", async (route) => {
    await json(route, {
      sessions: [
        {
          id: 1,
          started_at_ms: 1_716_000_000_000,
          ended_at_ms: null,
          status: "readonly_ready",
          execution_mode: "live_readonly",
          wallet_address: "0x1234",
          proxy_wallet: "0xabcd",
          enabled_strategies_json: "[\"latency-arb\",\"spread-capture\"]",
          config_fingerprint: "{\"execution_mode\":\"live_readonly\"}",
          cash_cap_usd: 100,
          details_json: null,
        },
      ],
    });
  });

  await page.route("**/api/bots/*/live/orders**", async (route) => {
    await json(route, { orders: [] });
  });

  await page.route("**/api/bots/*/live/fills**", async (route) => {
    await json(route, { fills: [] });
  });

  await page.route("**/api/bots/*/live/redemptions**", async (route) => {
    await json(route, { redemptions: [] });
  });

  await page.route("**/api/bots/*/live/reconciliation**", async (route) => {
    await json(route, { events: [] });
  });

  await page.route("**/api/bots/*/stats", async (route) => {
    await json(route, {
      by_strategy: {
        "latency-arb": {
          trades: 12,
          wins: 8,
          losses: 4,
          win_rate: 66.7,
          total_pnl: 50.5,
        },
      },
    });
  });

  await page.route("**/api/bots/*/process", async (route) => {
    await json(route, { active: true, pid: 1234, uptime_secs: 3600 });
  });

  await page.route("**/api/bots/*/start", async (route) => {
    await json(route, { active: true, pid: 1234, uptime_secs: 1 });
  });

  await page.route("**/api/bots/*/stop", async (route) => {
    await json(route, { active: false, pid: null, uptime_secs: null });
  });

  await page.route("**/api/bots/*/restart", async (route) => {
    await json(route, { active: true, pid: 2345, uptime_secs: 0 });
  });
}

const RESEARCH_TS = 1_779_000_000_000;
const RESEARCH_INTERVAL_END = 1_779_000_600_000;

function researchArtifactAvailable() {
  return {
    id: "fixture-artifact-available",
    source_machine_id: "fixture-live",
    kind: "readonly_run",
    run_mode: "live_readonly",
    artifact_root: "/research/artifacts/fixture-artifact-available",
    manifest_path:
      "/research/artifacts/fixture-artifact-available/manifest.json",
    bundle_path: null,
    source_db_path: "/runtime/paint.db",
    interval_start_ms: RESEARCH_TS,
    interval_end_ms: RESEARCH_INTERVAL_END,
    bytes: 33,
    checksum:
      "e7d8e9b5d6f4c3b2a190e8f7d6c5b4a3928171615141312110f0e0d0c0b0a090",
    replay_quality_class: "sweep_grade",
    backtest_ready_class: "backtest_ready",
    live_fidelity_class: "not_checked",
    status: "available",
    archived_at: null,
    created_at: RESEARCH_TS,
    updated_at: RESEARCH_TS,
  };
}

function researchStep(
  jobId: string,
  index: number,
  name: string,
  output: Record<string, unknown>,
) {
  return {
    id: `${jobId}-step-${index}`,
    job_id: jobId,
    step_index: index,
    name,
    status: "completed",
    lease_owner: null,
    leased_until_ms: null,
    attempts: 1,
    input_json: null,
    output_json: JSON.stringify(output),
    error: null,
    created_at: RESEARCH_TS,
    updated_at: RESEARCH_TS,
    started_at: RESEARCH_TS,
    completed_at: RESEARCH_TS,
  };
}

function researchCompletedJobDetail() {
  const jobId = "fixture-job-completed";
  const steps = [
    "verify_artifact",
    "validate_replay_data",
    "validate_backtest_input",
    "prepare_backtest_input",
    "run_backtest",
    "write_report",
  ].map((name, idx) =>
    researchStep(jobId, idx, name, { fixture_step: name, status: "completed" }),
  );
  return {
    job: {
      id: jobId,
      job_type: "current_params",
      artifact_id: "fixture-artifact-available",
      status: "completed",
      priority: 0,
      requested_by: "fixture-user-researcher",
      params_json: JSON.stringify({
        artifact_id: "fixture-artifact-available",
        balance: 200,
      }),
      created_at: RESEARCH_TS,
      updated_at: RESEARCH_TS,
      cancelled_at: null,
      completed_at: RESEARCH_TS,
    },
    steps,
    events: [
      {
        id: `${jobId}-event-0`,
        job_id: jobId,
        step_id: null,
        timestamp_ms: RESEARCH_TS,
        level: "info",
        message: "fixture job is completed",
        details_json: JSON.stringify({ fixture: true, status: "completed" }),
      },
    ],
  };
}

function researchCreatedJobDetail() {
  const detail = researchCompletedJobDetail();
  return {
    ...detail,
    job: {
      ...detail.job,
      id: "fixture-job-created",
      status: "queued",
      completed_at: null,
    },
    steps: detail.steps.map((step) => ({
      ...step,
      id: step.id.replace("fixture-job-completed", "fixture-job-created"),
      job_id: "fixture-job-created",
      status: "queued",
      attempts: 0,
      output_json: null,
      started_at: null,
      completed_at: null,
    })),
    events: [],
  };
}

function researchReport(id: string, artifactId: string) {
  return {
    id,
    job_id: "fixture-job-completed",
    artifact_id: artifactId,
    title: `Fixture Report ${id}`,
    status: "available",
    summary_json: JSON.stringify({
      schema_version: 2,
      net_pnl: id === "fixture-report-a" ? 284.25 : 142.5,
    }),
    report_path: `/research/jobs/${id}/${id}.json`,
    csv_path: `/research/jobs/${id}/${id}.csv`,
    created_at: RESEARCH_TS,
    updated_at: RESEARCH_TS,
  };
}

function researchReportPayload(id: string, artifactId: string) {
  const netPnl = id === "fixture-report-a" ? 284.25 : 142.5;
  return {
    schema_version: 2,
    provenance: {
      job_id: "fixture-job-completed",
      job_type: "current_params",
      artifact_id: artifactId,
      start: "2026-05-17T07:40:00.000Z",
      end: "2026-05-17T07:50:00.000Z",
      start_ms: RESEARCH_TS,
      end_ms: RESEARCH_INTERVAL_END,
      balance: 200,
    },
    metrics: {
      net_pnl: netPnl,
      max_drawdown: -91.4,
      win_rate: 0.58,
      trade_count: 43,
      final_balance: 200 + netPnl,
    },
    source_comparison: null,
    diagnostics: [],
    equity_curve: [
      { ts: RESEARCH_TS, equity: 200 },
      { ts: RESEARCH_TS + 300_000, equity: 200 + netPnl / 2 },
      { ts: RESEARCH_INTERVAL_END, equity: 200 + netPnl },
    ],
  };
}

function emptyResearchQueue() {
  return {
    generated_at_ms: RESEARCH_TS,
    counts: {
      jobs_total: 1,
      jobs_active: 0,
      jobs_waiting: 0,
      jobs_running: 0,
      jobs_retryable: 0,
      jobs_blocked: 0,
      jobs_failed: 0,
      jobs_completed: 1,
      stale_leases: 0,
      transfers_active: 0,
      transfers_attention: 0,
      disabled_hosts: 0,
    },
    jobs: {
      running: [],
      waiting: [],
      retryable: [],
      blocked: [],
      failed: [],
      stale_leases: [],
    },
    transfers: { active: [], attention: [], stale: [] },
    disabled_hosts: [],
    recent_reports: [
      researchReport("fixture-report-a", "fixture-artifact-available"),
    ],
    retention: {
      jobs: 0,
      reports: 0,
      artifacts: 0,
      scratch_bytes: 0,
      report_bytes: 0,
      artifact_bytes: 0,
    },
  };
}

export async function stubResearchApi(page: Page) {
  await page.route("**/api/research/machines", async (route) => {
    await json(route, { machines: [] });
  });
  await page.route("**/api/research/transfers", async (route) => {
    await json(route, { transfers: [] });
  });
  await page.route("**/api/research/artifacts", async (route) => {
    await json(route, { artifacts: [researchArtifactAvailable()] });
  });
  await page.route("**/api/research/job-templates", async (route) => {
    await json(route, { templates: [] });
  });
  await page.route("**/api/research/queue", async (route) => {
    await json(route, emptyResearchQueue());
  });
  await page.route("**/api/research/retention", async (route) => {
    await json(route, {
      generated_at_ms: RESEARCH_TS,
      jobs: [],
      reports: [],
      artifacts: [],
      totals: {
        jobs: 0,
        reports: 0,
        artifacts: 0,
        scratch_bytes: 0,
        report_bytes: 0,
        artifact_bytes: 0,
      },
    });
  });

  await page.route("**/api/research/jobs", async (route) => {
    if (route.request().method() === "POST") {
      await json(route, researchCreatedJobDetail());
      return;
    }
    await json(route, { jobs: [researchCompletedJobDetail().job] });
  });

  await page.route("**/api/research/reports", async (route) => {
    await json(route, {
      reports: [
        researchReport("fixture-report-a", "fixture-artifact-available"),
        researchReport("fixture-report-b", "fixture-artifact-other"),
      ],
    });
  });

  await page.route("**/api/research/jobs/*", async (route) => {
    const url = route.request().url();
    if (url.includes("fixture-job-created")) {
      await json(route, researchCreatedJobDetail());
      return;
    }
    await json(route, researchCompletedJobDetail());
  });

  await page.route("**/api/research/reports/*/json", async (route) => {
    const id = route.request().url().includes("fixture-report-b")
      ? "fixture-report-b"
      : "fixture-report-a";
    const artifact =
      id === "fixture-report-b"
        ? "fixture-artifact-other"
        : "fixture-artifact-available";
    await json(route, researchReportPayload(id, artifact));
  });

  await page.route("**/api/research/reports/*/csv", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "text/csv",
      body: "metric,value\nnet_pnl,284.25\n",
    });
  });

  await page.route("**/api/research/reports/*", async (route) => {
    const id = route.request().url().includes("fixture-report-b")
      ? "fixture-report-b"
      : "fixture-report-a";
    const artifact =
      id === "fixture-report-b"
        ? "fixture-artifact-other"
        : "fixture-artifact-available";
    await json(route, researchReport(id, artifact));
  });
}
