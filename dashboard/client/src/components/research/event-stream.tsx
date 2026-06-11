import { useMemo, useState } from "react";
import {
  Button,
  FormField,
  RelativeTime,
  Segment,
  StatusChip,
  Textarea,
} from "../ui/dashboard-primitives";
import { Dialog } from "../ui/dialog";
import { JsonViewer } from "./json-viewer";
import { StatusFilter } from "./status-filter";
import { humanize } from "../../lib/utils";
import type {
  AppendEventRequest,
  EventLevel,
  ResearchJobEvent,
} from "../../lib/research-types";
import type { ChipTone } from "../../lib/research-permissions";
import { canPerform } from "../../lib/research-permissions";

const ALL_LEVELS: EventLevel[] = ["info", "warn", "error", "progress", "debug"];

function levelTone(level: EventLevel): ChipTone {
  switch (level) {
    case "info":
      return "neutral";
    case "warn":
      return "warning";
    case "error":
      return "danger";
    case "progress":
      return "muted";
    case "debug":
    default:
      return "muted";
  }
}

interface EventStreamProps {
  events: ResearchJobEvent[];
  role: "admin" | "observer" | undefined;
  onAppend?: (req: AppendEventRequest) => Promise<void> | void;
  isAppending?: boolean;
  appendError?: string | null;
}

export function EventStream({
  events,
  role,
  onAppend,
  isAppending = false,
  appendError,
}: EventStreamProps) {
  const [active, setActive] = useState<string[]>([...ALL_LEVELS]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [noteOpen, setNoteOpen] = useState(false);
  const [noteLevel, setNoteLevel] = useState<EventLevel>("info");
  const [noteMessage, setNoteMessage] = useState("");

  const sorted = useMemo(
    () =>
      [...events]
        .filter((event) => active.includes(event.level))
        .sort((a, b) => b.timestamp_ms - a.timestamp_ms),
    [events, active],
  );

  const canAddNote = role ? canPerform(role, "append_event") : false;
  const closeNote = () => {
    if (isAppending) return;
    setNoteLevel("info");
    setNoteMessage("");
    setNoteOpen(false);
  };

  const submitNote = async () => {
    if (!onAppend) return;
    const message = noteMessage.trim();
    if (!message) return;
    try {
      await onAppend({ level: noteLevel, message });
      setNoteMessage("");
      setNoteOpen(false);
    } catch {
      return;
    }
  };

  return (
    <div className="space-y-2">
      <StatusFilter
        label="Level"
        statuses={ALL_LEVELS}
        active={active}
        onChange={setActive}
        toneFor={(s) => levelTone(s as EventLevel)}
        ariaLabel="Event level filter"
      />
      {onAppend && (
        <div className="flex justify-end">
          <Button
            size="sm"
            tone="accent"
            disabled={!canAddNote}
            title={canAddNote ? undefined : "Admin role required."}
            onClick={() => setNoteOpen(true)}
          >
            Add note
          </Button>
        </div>
      )}
      {sorted.length === 0 ? (
        <div className="text-[12px] text-muted">
          {events.length === 0
            ? "No events yet. Worker activity and operator notes appear here."
            : "No events match the selected levels."}
        </div>
      ) : (
        <ol className="space-y-1.5">
          {sorted.map((event) => {
            const isOpen = expanded === event.id;
            return (
              <li
                key={event.id}
                className="border border-border bg-bg"
              >
                <button
                  type="button"
                  onClick={() => setExpanded(isOpen ? null : event.id)}
                  className="flex w-full items-start gap-2 px-3 py-2 text-left"
                  aria-expanded={isOpen}
                >
                  <StatusChip
                    label={humanize(event.level)}
                    tone={levelTone(event.level)}
                    compact
                  />
                  <span className="min-w-0 flex-1 text-[12px] leading-snug">
                    {event.message}
                  </span>
                  <span className="shrink-0 text-[11px] text-muted">
                    <RelativeTime epochMs={event.timestamp_ms} />
                  </span>
                </button>
                {isOpen && event.details_json && (
                  <div className="border-t border-border px-3 py-2">
                    <JsonViewer
                      value={event.details_json}
                      label="Details"
                      maxHeight={200}
                    />
                  </div>
                )}
              </li>
            );
          })}
        </ol>
      )}
      <Dialog
        open={noteOpen}
        onClose={closeNote}
        title="Add operator note"
        description="Records an event on the job timeline."
        width="sm"
      >
        <div className="space-y-3">
          <FormField label="Level">
            {() => (
              <Segment
                value={noteLevel}
                onChange={(value) => setNoteLevel(value as EventLevel)}
                items={ALL_LEVELS.map((level) => ({
                  value: level,
                  label: level,
                }))}
                ariaLabel="Note level"
              />
            )}
          </FormField>
          <FormField label="Message" required>
            {({ id }) => (
              <Textarea
                id={id}
                value={noteMessage}
                onChange={(event) =>
                  setNoteMessage(event.currentTarget.value)
                }
                minRows={3}
                autoFocus
              />
            )}
          </FormField>
          {appendError && (
            <div className="text-[12px] text-accent-red">{appendError}</div>
          )}
          <div className="flex justify-end gap-2">
            <Button onClick={closeNote} disabled={isAppending}>
              Cancel
            </Button>
            <Button
              tone="accent"
              disabled={!noteMessage.trim() || isAppending}
              state={isAppending ? "pending" : "idle"}
              onClick={() => void submitNote()}
            >
              Save note
            </Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
