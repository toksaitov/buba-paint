import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StatusFilter } from "../status-filter";

function getTrigger() {
  return screen.getByRole("button", { name: /status filter/i });
}

async function openDisclosure() {
  await userEvent.click(getTrigger());
}

describe("StatusFilter", () => {
  it("renders as a closed disclosure trigger by default", () => {
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={["running"]}
        onChange={() => undefined}
      />,
    );
    const trigger = getTrigger();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("button", { name: /^queued$/i }),
    ).not.toBeInTheDocument();
  });

  it("opens a popover with one toggle per status on trigger click", async () => {
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={["running"]}
        onChange={() => undefined}
      />,
    );
    await openDisclosure();
    expect(getTrigger()).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: /^queued$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^running$/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^completed$/i }),
    ).toBeInTheDocument();
  });

  it("marks active statuses via aria-pressed when opened", async () => {
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running"]}
        active={["running"]}
        onChange={() => undefined}
      />,
    );
    await openDisclosure();
    expect(
      screen.getByRole("button", { name: /^running$/i }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: /^queued$/i }),
    ).toHaveAttribute("aria-pressed", "false");
  });

  it("toggles by calling onChange with the next set", async () => {
    const onChange = vi.fn();
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running"]}
        active={["running"]}
        onChange={onChange}
      />,
    );
    await openDisclosure();
    await userEvent.click(screen.getByRole("button", { name: /^queued$/i }));
    expect(onChange).toHaveBeenCalledWith(["running", "queued"]);
    await userEvent.click(screen.getByRole("button", { name: /^running$/i }));
    expect(onChange).toHaveBeenCalledWith([]);
  });

  it("summarises the active set in the trigger label", () => {
    const { rerender } = render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={["queued", "running", "completed"]}
        onChange={() => undefined}
      />,
    );
    expect(getTrigger().textContent).toMatch(/Status:\s*all/);
    rerender(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={[]}
        onChange={() => undefined}
      />,
    );
    expect(getTrigger().textContent).toMatch(/Status:\s*none/);
    rerender(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={["queued", "running"]}
        onChange={() => undefined}
      />,
    );
    expect(getTrigger().textContent).toMatch(/Queued, Running/);
  });

  it("humanises underscored status names in chips by default", async () => {
    render(
      <StatusFilter
        label="Status"
        statuses={["not_configured", "online"]}
        active={["online"]}
        onChange={() => undefined}
      />,
    );
    await openDisclosure();
    expect(
      screen.getByRole("button", { name: /Not configured/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Online/i })).toBeInTheDocument();
  });

  it("exposes select-all and clear shortcuts when expanded", async () => {
    const onChange = vi.fn();
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running", "completed"]}
        active={["running"]}
        onChange={onChange}
      />,
    );
    await openDisclosure();
    await userEvent.click(screen.getByRole("button", { name: /^all$/i }));
    expect(onChange).toHaveBeenLastCalledWith([
      "queued",
      "running",
      "completed",
    ]);
    await userEvent.click(screen.getByRole("button", { name: /^clear$/i }));
    expect(onChange).toHaveBeenLastCalledWith([]);
  });

  it("closes the popover on Escape", async () => {
    render(
      <StatusFilter
        label="Status"
        statuses={["queued", "running"]}
        active={["running"]}
        onChange={() => undefined}
      />,
    );
    await openDisclosure();
    expect(getTrigger()).toHaveAttribute("aria-expanded", "true");
    await userEvent.keyboard("{Escape}");
    expect(getTrigger()).toHaveAttribute("aria-expanded", "false");
  });
});
