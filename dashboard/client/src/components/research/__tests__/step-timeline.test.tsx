import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { StepTimeline } from "../step-timeline";
import { fixtureJobRunning } from "../../../lib/research-fixtures";

describe("StepTimeline", () => {
  it("shows an empty message when there are no steps", () => {
    render(<StepTimeline steps={[]} role="admin" />);
    expect(screen.getByText("No steps recorded yet.")).toBeInTheDocument();
  });

  it("renders step names and a status chip for the active step", () => {
    const { steps } = fixtureJobRunning();
    render(
      <StepTimeline steps={steps} role="observer" nowMs={steps[0].leased_until_ms ?? 0} />,
    );
    expect(screen.getByText("Verify artifact")).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
  });

  it("flags an overdue lease on the active step", () => {
    const { steps } = fixtureJobRunning();
    render(
      <StepTimeline
        steps={steps}
        role="admin"
        nowMs={Number.MAX_SAFE_INTEGER}
      />,
    );
    expect(screen.getByText(/refresh overdue by/i)).toBeInTheDocument();
  });
});
