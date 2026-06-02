import { useState } from "react";
import {
  Banner,
  Button,
  FormField,
  Input,
} from "../ui/dashboard-primitives";
import { Dialog } from "../ui/dialog";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  phrase?: string;
  destructive?: boolean;
  pending?: boolean;
  errorMessage?: string;
  onConfirm: () => void;
  onClose: () => void;
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  phrase,
  destructive = false,
  pending = false,
  errorMessage,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  const [typed, setTyped] = useState("");

  const requiresPhrase = phrase != null && phrase.length > 0;
  const phraseSatisfied = !requiresPhrase || typed === phrase;
  const canConfirm = !pending && phraseSatisfied;

  const closeAndReset = () => {
    setTyped("");
    onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={pending ? () => undefined : closeAndReset}
      title={title}
      description={description}
      width="sm"
    >
      <div className="space-y-3">
        {errorMessage && (
          <Banner tone="danger" title="Action failed">
            {errorMessage}
          </Banner>
        )}
        {requiresPhrase && (
          <FormField
            label={`Type "${phrase}" to confirm`}
            hint="This action cannot be undone from the dashboard."
          >
            {({ id }) => (
              <Input
                id={id}
                value={typed}
                onChange={(event) => setTyped(event.currentTarget.value)}
                state={
                  typed.length === 0
                    ? "idle"
                    : phraseSatisfied
                      ? "valid"
                      : "invalid"
                }
                autoFocus
              />
            )}
          </FormField>
        )}
        <div className="flex justify-end gap-2">
          <Button
            tone="neutral"
            onClick={closeAndReset}
            disabled={pending}
          >
            Cancel
          </Button>
          <Button
            tone={destructive ? "danger" : "accent"}
            onClick={onConfirm}
            disabled={!canConfirm}
            state={pending ? "pending" : "idle"}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
