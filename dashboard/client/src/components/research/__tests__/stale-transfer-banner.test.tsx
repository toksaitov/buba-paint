import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { StaleTransferBanner } from "../stale-transfer-banner";
import { TRANSFER_STALE_MS } from "../../../lib/research-types";
import type { ArtifactTransfer } from "../../../lib/research-types";

function makeTransfer(
  overrides: Partial<ArtifactTransfer> = {},
): ArtifactTransfer {
  return {
    id: "t",
    artifact_id: "a",
    source_machine_id: "live",
    dest_machine_id: "research",
    status: "running",
    bytes_total: 1000,
    bytes_done: 400,
    checksum_status: "pending",
    error: null,
    created_at: 0,
    updated_at: 0,
    completed_at: null,
    ...overrides,
  };
}

describe("StaleTransferBanner", () => {
  it("renders for running transfers older than stale threshold", () => {
    const now = TRANSFER_STALE_MS + 60_000;
    render(
      <StaleTransferBanner
        transfer={makeTransfer({ status: "running", updated_at: 0 })}
        nowMs={now}
      />,
    );
    expect(
      screen.getByText(/transfer may have stalled/i),
    ).toBeInTheDocument();
  });

  it("does not render for fresh running transfers", () => {
    render(
      <StaleTransferBanner
        transfer={makeTransfer({ updated_at: 1000 })}
        nowMs={2000}
      />,
    );
    expect(
      screen.queryByText(/transfer may have stalled/i),
    ).not.toBeInTheDocument();
  });

  it("does not render for non-running transfers", () => {
    render(
      <StaleTransferBanner
        transfer={makeTransfer({ status: "paused", updated_at: 0 })}
        nowMs={TRANSFER_STALE_MS + 60_000}
      />,
    );
    expect(
      screen.queryByText(/transfer may have stalled/i),
    ).not.toBeInTheDocument();
  });
});
