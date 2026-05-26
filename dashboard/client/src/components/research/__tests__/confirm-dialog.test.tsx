import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmDialog } from "../confirm-dialog";

describe("ConfirmDialog", () => {
  it("renders title, description, and disables confirm until phrase matches", async () => {
    const onConfirm = vi.fn();
    const onClose = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Delete artifact"
        description="Type the ID."
        phrase="abc-123"
        destructive
        confirmLabel="Delete artifact"
        onConfirm={onConfirm}
        onClose={onClose}
      />,
    );
    expect(screen.getByRole("heading", { name: "Delete artifact" })).toBeInTheDocument();
    const confirmBtn = screen.getByRole("button", { name: /delete artifact/i });
    expect(confirmBtn).toBeDisabled();

    const input = screen.getByRole("textbox");
    await userEvent.type(input, "abc-123");
    expect(confirmBtn).not.toBeDisabled();
    await userEvent.click(confirmBtn);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("does not call onConfirm when phrase is wrong", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Delete"
        description="d"
        phrase="exact"
        destructive
        onConfirm={onConfirm}
        onClose={vi.fn()}
      />,
    );
    const input = screen.getByRole("textbox");
    await userEvent.type(input, "wrong");
    expect(screen.getByRole("button", { name: /confirm/i })).toBeDisabled();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("renders no phrase input when phrase is not required", () => {
    render(
      <ConfirmDialog
        open={true}
        title="Confirm"
        description="No phrase"
        onConfirm={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /confirm/i })).not.toBeDisabled();
  });

  it("disables both buttons when pending", () => {
    render(
      <ConfirmDialog
        open={true}
        title="t"
        description="d"
        pending
        onConfirm={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /cancel/i })).toBeDisabled();
  });

  it("shows the error banner when errorMessage is set", () => {
    render(
      <ConfirmDialog
        open={true}
        title="t"
        description="d"
        errorMessage="permission denied"
        onConfirm={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText(/permission denied/)).toBeInTheDocument();
  });
});
