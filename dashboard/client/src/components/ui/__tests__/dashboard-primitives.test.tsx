import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import {
  AlertList,
  ContextStrip,
  KeyValueList,
  MetricCard,
  PageHeader,
  SectionCard,
  StateEmpty,
  StatusChip,
  Surface,
  TableToolbar,
} from "../dashboard-primitives";

describe("dashboard primitives", () => {
  test("renders status chips with dots and titles", () => {
    render(<StatusChip label="Running" tone="success" dot title="Process running" />);
    expect(screen.getByText("Running")).toHaveAttribute("title", "Process running");
  });

  test("renders context strip and section shell", () => {
    render(
      <>
        <ContextStrip
          title="Trading"
          description="Polymarket account state."
        />
        <PageHeader title="Trades" description="Shadow trade history." />
        <SectionCard title="Account" subtitle="Details" toolbar={<div>toolbar</div>}>
          <div>content</div>
        </SectionCard>
        <Surface>
          <div>surface</div>
        </Surface>
      </>,
    );

    expect(screen.getByText("Trading")).toBeInTheDocument();
    expect(screen.getByText("Polymarket account state.")).toBeInTheDocument();
    expect(screen.getByText("Trades")).toBeInTheDocument();
    expect(screen.getByText("Shadow trade history.")).toBeInTheDocument();
    expect(screen.getByText("toolbar")).toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
    expect(screen.getByText("surface")).toBeInTheDocument();
  });

  test("renders metric, key-value, toolbar, and empty states", () => {
    render(
      <>
        <MetricCard label="Available Cash" value="$99.17" tone="warning" sub="Allowance unknown" />
        <KeyValueList
          columns={2}
          items={[
            { label: "Account", value: "$99.17", tone: "success" },
            { label: "Reconciliation", value: "Critical", tone: "danger" },
            { label: "Venue", value: "Warning", tone: "warning" },
          ]}
        />
        <TableToolbar left={<div>left</div>} right={<div>right</div>} />
        <StateEmpty message="No data." />
      </>,
    );

    expect(screen.getByText("Available Cash")).toBeInTheDocument();
    expect(screen.getByText("Allowance unknown")).toBeInTheDocument();
    expect(screen.getByText("left")).toBeInTheDocument();
    expect(screen.getByText("right")).toBeInTheDocument();
    expect(screen.getByText("No data.")).toBeInTheDocument();
  });

  test("renders alert lists for empty, warning, and critical states", () => {
    const { rerender } = render(<AlertList alerts={[]} emptyMessage="Nothing here." />);
    expect(screen.getByText("Nothing here.")).toBeInTheDocument();

    rerender(
      <AlertList
        alerts={[
          { severity: "warning", title: "Readonly degraded", detail: "User stream stale." },
          { severity: "critical", title: "Kill switch", detail: "Trading disarmed." },
        ]}
      />,
    );

    expect(screen.getByText("Readonly degraded")).toBeInTheDocument();
    expect(screen.getByText("Kill switch")).toBeInTheDocument();
    expect(screen.getByText("warning")).toBeInTheDocument();
    expect(screen.getByText("critical")).toBeInTheDocument();
  });
});
