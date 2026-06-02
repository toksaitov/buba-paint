import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { JobCreateForm } from "../job-create-form";
import type { ResearchArtifact } from "../../../lib/research-types";

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
    interval_start_ms: 1_000,
    interval_end_ms: 2_000,
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

describe("JobCreateForm - export", () => {
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
  it("requires at least one sweep dimension to enable submit", async () => {
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
    expect(createBtn).toBeDisabled();

    await userEvent.click(
      screen.getByRole("button", { name: /add sweep dimension/i }),
    );

    const keyInput = screen.getByPlaceholderText("parameter");
    const valueInput = screen.getByPlaceholderText("1.0,2.5,4.0");
    await userEvent.type(keyInput, "K");
    await userEvent.type(valueInput, "1.0,2.0");

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: /confirm this interval before creating the job/i,
      }),
    );

    expect(createBtn).not.toBeDisabled();
    await userEvent.click(createBtn);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.job_type).toBe("sweep");
    expect(payload.artifact_id).toBe("art-1");
    expect(payload.params.sweeps).toContain("K=1.0,2.0");
  });
});

describe("JobCreateForm - backtest", () => {
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
    const start = "2026-05-17T07:39";
    const end = "2026-05-17T07:41";
    fireEvent.change(screen.getByLabelText(/^Start/i), {
      target: { value: start },
    });
    fireEvent.change(screen.getByLabelText(/^End/i), {
      target: { value: end },
    });

    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).not.toBeDisabled();
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
    expect(screen.getByText(/effective interval/i)).toBeInTheDocument();
    expect(screen.getAllByText(/typed value/i).length).toBeGreaterThan(0);
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
    const start = "2026-05-17T07:39";
    const end = "2026-05-17T07:41";
    const startInput = screen.getByLabelText(/^Start/i) as HTMLInputElement;
    const endInput = screen.getByLabelText(/^End/i) as HTMLInputElement;
    startInput.value = start;
    fireEvent.blur(startInput);
    endInput.value = end;
    fireEvent.blur(endInput);

    const createBtn = screen.getByRole("button", { name: /create job/i });
    expect(createBtn).not.toBeDisabled();
    await userEvent.click(createBtn);

    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.start_ms).toBe(Date.parse(start));
    expect(payload.params.end_ms).toBe(Date.parse(end));
  });

  it("requires confirmation for artifact fallback intervals", async () => {
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
    expect(createBtn).toBeDisabled();
    expect(
      screen.getAllByText(/leave blank to use artifact interval/i).length,
    ).toBe(2);
    expect(
      screen.getByText(/start and end are blank, so this job will use the artifact interval/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/start source/i)).toBeInTheDocument();
    expect(screen.getByText(/end source/i)).toBeInTheDocument();
    expect(screen.getAllByText(/artifact interval/i).length).toBeGreaterThan(1);
    expect(screen.queryByText(/artifact fallback/i)).not.toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: /confirm this interval before creating the job/i,
      }),
    );

    expect(createBtn).not.toBeDisabled();
    await userEvent.click(createBtn);
    const payload = onSubmit.mock.calls[0][0];
    expect(payload.params.start_ms).toBe(1_000);
    expect(payload.params.end_ms).toBe(2_000);
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
    expect(screen.getByText(/start and end are required/i)).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/^Start/i), {
      target: { value: "2026-05-17T09:00" },
    });
    fireEvent.change(screen.getByLabelText(/^End/i), {
      target: { value: "2026-05-17T08:00" },
    });
    expect(createBtn).toBeDisabled();
    expect(screen.getByText(/end must be after start/i)).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/^Start/i), {
      target: { value: "2026-05-17T00:00" },
    });
    fireEvent.change(screen.getByLabelText(/^End/i), {
      target: { value: "2026-05-17T07:01" },
    });
    expect(createBtn).toBeDisabled();

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: /confirm this interval before creating the job/i,
      }),
    );
    expect(createBtn).not.toBeDisabled();
  });
});
