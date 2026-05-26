import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { RoleGate } from "../role-gate";

describe("RoleGate", () => {
  it("renders children for admin on mutate actions", () => {
    render(
      <RoleGate role="admin" action="create">
        <button>New job</button>
      </RoleGate>,
    );
    expect(screen.getByRole("button", { name: /new job/i })).toBeInTheDocument();
  });

  it("renders admin-required banner for observer", () => {
    render(
      <RoleGate role="observer" action="create" message="Custom message.">
        <button>New job</button>
      </RoleGate>,
    );
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.getByText(/admin role required/i)).toBeInTheDocument();
    expect(screen.getByText(/custom message/i)).toBeInTheDocument();
  });

  it("renders banner when role is undefined", () => {
    render(
      <RoleGate role={undefined} action="create">
        <button>New job</button>
      </RoleGate>,
    );
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders children for read actions for observer", () => {
    render(
      <RoleGate role="observer" action="read">
        <span>visible</span>
      </RoleGate>,
    );
    expect(screen.getByText("visible")).toBeInTheDocument();
  });
});
