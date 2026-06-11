import { describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { JobCreateForm } from "../job-create-form";
import type { ResearchArtifact } from "../../../lib/research-types";

const ARTIFACT_START_MS = Date.parse("2026-05-17T07:00");
const ARTIFACT_END_MS = Date.parse("2026-05-17T08:00");

function makeArtifact(
  overrides: Partial<ResearchArtifact> = {},
): ResearchArtifact {
  return {
    id: "art-1",
    source_machine_id: "live",
    kind: "runtime_export",
    status: "available",
    run_mode: "paper",
    artifact_root: "/r/a",
    manifest_path: "/r/a/manifest.json",
    bundle_path: null,
    source_db_path: "/r/a/paint.db",
    interval_start_ms: ARTIFACT_START_MS,
    interval_end_ms: ARTIFACT_END_MS,
    bytes: 1024,
    checksum: "deadbeef",
    replay_quality_class: "A",
    backtest_ready_class: "ready",
    live_fidelity_class: "high",
    created_at: 0,
    updated_at: 0,
    archived_at: null,
    ...overrides,
  };
}

function switchToCustomRange() {
  const intervalGroup = screen.getByRole("radiogroup", {
    name: /replay interval/i,
  });
  fireEvent.click(
    within(intervalGroup).getByRole("radio", { name: /custom range/i }),
  );
}

const startDateLabel = /^start(\s*required)?$/i;
const endDateLabel = /^end(\s*required)?$/i;

describe("JobCreateForm - export", () => {
  it("shows available artifacts as direct backtest and sweep actions", async () => {
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "live-artifact" })]}
        initialType="export"
        pending={false}
        error={null}
        onSubmit={vi.fn()}
      />,
    );

    expect(screen.getByText(/available run artifacts/i)).toBeInTheDocument();
    expect(screen.getByText("live-artifact")).toBeInTheDocument();
    expect(
      screen.queryByText(/detected stopped-run export is not automatic/i),
    ).not.toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: /backtest live-artifact/i }),
    );

    expect(screen.getByText(/backtest one parameter set/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/artifact to replay/i)).toHaveValue(
      "live-artifact",
    );
  });

  it("blocks live_trading export with danger banner and disables submit", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[]}
        initialType="export"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    const sourceInput = screen.getByPlaceholderText(/paint\.db/);
    await userEvent.type(sourceInput, "/x/y.db");

    const liveTrading = screen.getByRole("radio", { name: "live_trading" });
    await userEvent.click(liveTrading);

    expect(
      screen.getByText(/export blocked for live_trading/i),
    ).toBeInTheDocument();
    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).toBeDisabled();
  });

  it("submits a dry_run export when valid", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[]}
        initialType="export"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    const sourceInput = screen.getByPlaceholderText(/paint\.db/);
    await userEvent.type(sourceInput, "/x/paint.db");
    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).not.toBeDisabled();
    await userEvent.click(createBtn);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.job_type).toBe("export");
    expect(payload.params.dry_run).toBe(true);
    expect(payload.params.source_db_path).toBe("/x/paint.db");
  });

  it("non-dry-run export requires confirm_export to enable submit", async () => {
    render(
      <JobCreateForm
        artifacts={[]}
        initialType="export"
        pending={false}
        error={null}
        onSubmit={vi.fn()}
      />,
    );
    const sourceInput = screen.getByPlaceholderText(/paint\.db/);
    await userEvent.type(sourceInput, "/x/paint.db");

    const dryRun = screen.getByRole("checkbox", { name: /dry run only/i });
    await userEvent.click(dryRun);

    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).toBeDisabled();

    const confirm = screen.getByRole("checkbox", {
      name: /i understand and want to perform a real export/i,
    });
    await userEvent.click(confirm);
    expect(createBtn).not.toBeDisabled();
  });
});

describe("JobCreateForm - sweep", () => {
  it("submits full preset sweep dimensions with the full artifact interval", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-1" })]}
        initialType="sweep"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );

    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).not.toBeDisabled();
    expect(screen.getAllByText(/full sweep preset/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/48 parameter combinations/i)).toBeInTheDocument();

    await userEvent.click(createBtn);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.job_type).toBe("sweep");
    expect(payload.artifact_id).toBe("art-1");
    expect(payload.params.sweep_scope).toBe("full");
    expect(payload.params.interval_mode).toBe("artifact");
    expect(payload.params.start_ms).toBe(ARTIFACT_START_MS);
    expect(payload.params.end_ms).toBe(ARTIFACT_END_MS);
    expect(payload.params.sweeps).toContain(
      "LATENCY_ARB_MIN_ASK=0.25,0.30,0.35,0.40",
    );
  });

  it("supports focused sweep ranges with named parameters", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-1" })]}
        initialType="sweep"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.click(
      screen.getByRole("radio", { name: /focused ranges/i }),
    );
    const firstValue = screen.getByLabelText(
      "Sweep dimensions value 1",
    ) as HTMLInputElement;
    await userEvent.clear(firstValue);
    await userEvent.type(firstValue, "0.30,0.35");

    await userEvent.click(screen.getByRole("button", { name: /create job/i }));
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.sweep_scope).toBe("focused");
    expect(payload.params.sweeps).toContain("LATENCY_ARB_MIN_ASK=0.30,0.35");
  });
});

describe("JobCreateForm - backtest", () => {
  it("defaults starting balance to the live paper baseline", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByLabelText(/starting balance/i)).toHaveValue("100");

    await userEvent.click(screen.getByRole("button", { name: /create job/i }));
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.balance).toBe(100);
  });

  it("submits explicit start/end from datetime-local inputs", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    switchToCustomRange();
    const start = "2026-05-17T07:39";
    const end = "2026-05-17T07:41";
    fireEvent.change(await screen.findByLabelText(startDateLabel), {
      target: { value: start },
    });
    fireEvent.change(await screen.findByLabelText(endDateLabel), {
      target: { value: end },
    });

    const createBtn = screen.getByRole("button", { name: /create job/i });
    await waitFor(() => expect(createBtn).not.toBeDisabled());
    await userEvent.click(createBtn);
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        job_type: "current_params",
        artifact_id: "art-77",
      }),
    );
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.start_ms).toBe(Date.parse(start));
    expect(payload.params.end_ms).toBe(Date.parse(end));
    expect(payload.params.param_source).toBe("worker_defaults");
    expect(payload.params.interval_mode).toBe("custom");
    expect(payload.params.set).toBeUndefined();
    expect(screen.getByText(/job interval/i)).toBeInTheDocument();
    expect(screen.getAllByText(/set by you/i).length).toBeGreaterThan(0);
  });

  it("submits named custom parameter overrides for backtests", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    switchToCustomRange();
    fireEvent.change(await screen.findByLabelText(startDateLabel), {
      target: { value: "2026-05-17T07:39" },
    });
    fireEvent.change(await screen.findByLabelText(endDateLabel), {
      target: { value: "2026-05-17T07:41" },
    });
    fireEvent.click(screen.getByRole("radio", { name: /custom settings/i }));
    await userEvent.type(
      screen.getByLabelText("Parameter overrides value 1"),
      "0.31",
    );

    const createBtn = screen.getByRole("button", { name: /create job/i });
    await waitFor(() => expect(createBtn).not.toBeDisabled());
    await userEvent.click(createBtn);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.param_source).toBe("custom");
    expect(payload.params.set).toContain("LATENCY_ARB_MIN_ASK=0.31");
  });

  it("renders custom parameter controls with readable full-width fields", async () => {
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: /custom settings/i }));

    const parameterSelect = screen.getByLabelText(
      "Parameter overrides parameter 1",
    );
    expect(parameterSelect).toHaveClass("min-h-[40px]");
    expect(parameterSelect).toHaveClass("w-full");
    expect(screen.getByText("Parameter")).toBeInTheDocument();
    expect(screen.getByText("Value or range")).toBeInTheDocument();
  });

  it("syncs browser-filled datetime-local values on blur", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    switchToCustomRange();
    const start = "2026-05-17T07:39";
    const end = "2026-05-17T07:41";
    const startInput = (await screen.findByLabelText(
      startDateLabel,
    )) as HTMLInputElement;
    const endInput = (await screen.findByLabelText(
      endDateLabel,
    )) as HTMLInputElement;
    startInput.value = start;
    fireEvent.blur(startInput);
    endInput.value = end;
    fireEvent.blur(endInput);

    const createBtn = screen.getByRole("button", { name: /create job/i });
    await waitFor(() => expect(createBtn).not.toBeDisabled());
    await userEvent.click(createBtn);

    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.start_ms).toBe(Date.parse(start));
    expect(payload.params.end_ms).toBe(Date.parse(end));
  });

  it("uses full artifact intervals without blank date-field fallback", async () => {
    const onSubmit = vi.fn();
    render(
      <JobCreateForm
        artifacts={[makeArtifact({ id: "art-77" })]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={onSubmit}
      />,
    );
    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).not.toBeDisabled();
    expect(screen.queryByLabelText(startDateLabel)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(endDateLabel)).not.toBeInTheDocument();
    expect(screen.queryByText(/blank uses the artifact/i)).not.toBeInTheDocument();
    expect(screen.getByText(/start source/i)).toBeInTheDocument();
    expect(screen.getByText(/end source/i)).toBeInTheDocument();
    expect(screen.getAllByText(/^artifact$/i).length).toBeGreaterThan(1);

    await userEvent.click(createBtn);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.interval_mode).toBe("artifact");
    expect(payload.params.start_ms).toBe(ARTIFACT_START_MS);
    expect(payload.params.end_ms).toBe(ARTIFACT_END_MS);
  });

  it("blocks invalid, missing, reversed, and unconfirmed large intervals", async () => {
    render(
      <JobCreateForm
        artifacts={[
          makeArtifact({
            id: "art-77",
            interval_start_ms: null,
            interval_end_ms: null,
          }),
        ]}
        initialType="current_params"
        pending={false}
        error={null}
        onSubmit={vi.fn()}
      />,
    );
    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).toBeDisabled();
    expect(
      screen.getByText(/does not include a usable interval/i),
    ).toBeInTheDocument();

    switchToCustomRange();
    expect(
      await screen.findByText(/custom start and end are required/i),
    ).toBeInTheDocument();

    fireEvent.change(await screen.findByLabelText(startDateLabel), {
      target: { value: "2026-05-17T09:00" },
    });
    fireEvent.change(await screen.findByLabelText(endDateLabel), {
      target: { value: "2026-05-17T08:00" },
    });
    expect(createBtn).toBeDisabled();
    expect(screen.getByText(/end must be after start/i)).toBeInTheDocument();

    fireEvent.change(await screen.findByLabelText(startDateLabel), {
      target: { value: "2026-05-17T00:00" },
    });
    fireEvent.change(await screen.findByLabelText(endDateLabel), {
      target: { value: "2026-05-17T07:01" },
    });
    expect(createBtn).toBeDisabled();

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: /confirm this long interval/i,
      }),
    );
    await waitFor(() => expect(createBtn).not.toBeDisabled());
  });
});
