import { useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType } from "react";
import { useOutletContext } from "react-router-dom";
import AnsiModule from "ansi-to-react";
import { Loading } from "../components/common/loading";
import { StateEmpty, Surface, TableToolbar } from "../components/ui/dashboard-primitives";
import { useLogs } from "../hooks/use-logs";
import { empty } from "../lib/copy";
import { cn } from "../lib/utils";

type LogSeverity = "error" | "warn" | "info" | "other";
type LogSource =
  | "live"
  | "live_readonly"
  | "market_discovery"
  | "clob"
  | "chainlink"
  | "binance"
  | "settlement"
  | "other";
type LogEventType =
  | "rollups"
  | "feed_events"
  | "market_discovery"
  | "settlement"
  | "readonly"
  | "errors"
  | "warnings"
  | "other";

interface ParsedLogLine {
  raw: string;
  severity: LogSeverity;
  source: LogSource;
  sourceLabel: string;
  message: string;
  eventType: LogEventType;
}

const sourceOptions: Array<{ value: "all" | LogSource; label: string }> = [
  { value: "all", label: "All sources" },
  { value: "live", label: "Live" },
  { value: "live_readonly", label: "Live readonly" },
  { value: "market_discovery", label: "Market discovery" },
  { value: "clob", label: "CLOB" },
  { value: "chainlink", label: "Chainlink" },
  { value: "binance", label: "Binance" },
  { value: "settlement", label: "Settlement" },
];

const eventTypeOptions: Array<{ value: "all" | LogEventType; label: string }> = [
  { value: "all", label: "All event types" },
  { value: "rollups", label: "Rollups" },
  { value: "feed_events", label: "Feed events" },
  { value: "market_discovery", label: "Market discovery" },
  { value: "settlement", label: "Settlement" },
  { value: "readonly", label: "Readonly" },
  { value: "warnings", label: "Warnings" },
  { value: "errors", label: "Errors" },
];

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let ansiImpl: any = AnsiModule;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
while (typeof ansiImpl !== "function" && ansiImpl && typeof ansiImpl.default !== "undefined") {
  ansiImpl = ansiImpl.default;
}

const Ansi: ComponentType<{ children: string }> = ansiImpl;

function lineSeverity(line: string): LogSeverity {
  if (line.includes(" ERROR ")) return "error";
  if (line.includes(" WARN ")) return "warn";
  if (line.includes(" INFO ")) return "info";
  return "other";
}

function classifySource(sourceToken: string, line: string): LogSource {
  const haystack = `${sourceToken} ${line}`.toLowerCase();
  if (haystack.includes("market_discovery")) return "market_discovery";
  if (haystack.includes("live_readonly")) return "live_readonly";
  if (haystack.includes("chainlink")) return "chainlink";
  if (haystack.includes("clob")) return "clob";
  if (haystack.includes("binance")) return "binance";
  if (haystack.includes("settlement") || haystack.includes("resolved")) return "settlement";
  if (haystack.includes("live")) return "live";
  return "other";
}

function classifyEventType(
  severity: LogSeverity,
  source: LogSource,
  message: string,
  raw: string,
): LogEventType {
  if (severity === "error") return "errors";
  if (severity === "warn") return "warnings";

  const haystack = `${source} ${message} ${raw}`.toLowerCase();
  if (haystack.includes("rollup")) return "rollups";
  if (
    haystack.includes("market_discovery") ||
    haystack.includes("discovered new window") ||
    haystack.includes("window activation scheduled") ||
    haystack.includes("fetching https://gamma-api")
  ) {
    return "market_discovery";
  }
  if (
    haystack.includes("settlement") ||
    haystack.includes("awaiting authoritative resolution") ||
    haystack.includes("resolved") ||
    haystack.includes("redeem")
  ) {
    return "settlement";
  }
  if (
    haystack.includes("feed connected") ||
    haystack.includes("feed disconnected") ||
    haystack.includes("reconnecting") ||
    haystack.includes("resubscrib") ||
    haystack.includes("ws://") ||
    haystack.includes("wss://")
  ) {
    return "feed_events";
  }
  if (haystack.includes("readonly")) return "readonly";
  return "other";
}

function parseLogLine(line: string): ParsedLogLine {
  const severity = lineSeverity(line);
  const match = line.match(/\b(?:INFO|WARN|ERROR)\s+([A-Za-z0-9_:]+):?\s*(.*)$/);
  const sourceToken = match?.[1]?.replace(/:$/, "") ?? "unknown";
  const message = match?.[2]?.trim() ?? line;
  const source = classifySource(sourceToken, line);

  return {
    raw: line,
    severity,
    source,
    sourceLabel: sourceToken,
    message,
    eventType: classifyEventType(severity, source, message, line),
  };
}

function lineTone(line: ParsedLogLine) {
  if (
    line.raw.includes("feed disconnected") ||
    line.raw.includes("reconnecting") ||
    line.raw.includes("No market data")
  ) {
    return "border-accent-red/50";
  }
  if (line.raw.includes("feed connected") || line.raw.includes("resubscrib")) {
    return "border-border";
  }
  if (line.raw.includes("readonly shadow runtime rollup")) {
    return "border-accent-blue/50";
  }
  if (line.severity === "error") {
    return "border-accent-red";
  }
  if (line.severity === "warn" || line.eventType === "readonly") {
    return "border-accent-blue/50";
  }
  if (line.eventType === "rollups") {
    return "border-border";
  }
  return "border-transparent";
}

export function LogsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [lines, setLines] = useState(200);
  const [follow, setFollow] = useState(true);
  const [search, setSearch] = useState("");
  const [severity, setSeverity] = useState<"all" | LogSeverity>("all");
  const [source, setSource] = useState<"all" | LogSource>("all");
  const [eventType, setEventType] = useState<"all" | LogEventType>("all");
  const [wrap, setWrap] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const { data, isLoading } = useLogs(botId, lines);
  const bottomRef = useRef<HTMLDivElement>(null);

  const activeFilterCount =
    (severity !== "all" ? 1 : 0) + (source !== "all" ? 1 : 0) + (eventType !== "all" ? 1 : 0);

  const parsedLines = useMemo(
    () => (data?.lines ?? []).map((line) => parseLogLine(line)),
    [data?.lines],
  );
  const eventCounts = useMemo(() => {
    const counts = new Map<LogEventType, number>();
    for (const line of parsedLines) {
      counts.set(line.eventType, (counts.get(line.eventType) ?? 0) + 1);
    }
    return counts;
  }, [parsedLines]);

  const filteredLines = useMemo(() => {
    return parsedLines.filter((line) => {
      if (severity !== "all" && line.severity !== severity) {
        return false;
      }
      if (source !== "all" && line.source !== source) {
        return false;
      }
      if (eventType !== "all" && line.eventType !== eventType) {
        return false;
      }
      if (search.trim()) {
        const query = search.trim().toLowerCase();
        if (
          !line.raw.toLowerCase().includes(query) &&
          !line.message.toLowerCase().includes(query) &&
          !line.sourceLabel.toLowerCase().includes(query)
        ) {
          return false;
        }
      }
      return true;
    });
  }, [eventType, parsedLines, search, severity, source]);

  useEffect(() => {
    if (follow && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [filteredLines, follow]);

  if (isLoading) return <Loading label="Loading logs" />;

  return (
    <Surface className="overflow-hidden bg-[#0a0a0a]">
        <TableToolbar
          left={
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search log lines"
                className="w-full border border-border bg-bg px-2 py-1 text-[11px] md:w-52"
              />
              <button
                type="button"
                onClick={() => setFiltersOpen((open) => !open)}
                className="border border-border bg-bg px-2 py-1 text-[11px] md:hidden"
              >
                {activeFilterCount > 0
                  ? `Filters (${activeFilterCount} active)`
                  : "Filters"}
              </button>
              <div className="hidden flex-wrap items-center gap-2 md:flex">
                <select
                  aria-label="Severity"
                  value={severity}
                  onChange={(event) => setSeverity(event.target.value as "all" | LogSeverity)}
                  className="border border-border bg-bg px-2 py-1 text-[11px]"
                >
                  <option value="all">All severities</option>
                  <option value="error">Errors</option>
                  <option value="warn">Warnings</option>
                  <option value="info">Info</option>
                </select>
                <select
                  aria-label="Source"
                  value={source}
                  onChange={(event) => setSource(event.target.value as "all" | LogSource)}
                  className="border border-border bg-bg px-2 py-1 text-[11px]"
                >
                  {sourceOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Event type"
                  value={eventType}
                  onChange={(event) =>
                    setEventType(event.target.value as "all" | LogEventType)
                  }
                  className="border border-border bg-bg px-2 py-1 text-[11px]"
                >
                  {eventTypeOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.value === "all"
                        ? option.label
                        : `${option.label} (${eventCounts.get(option.value) ?? 0})`}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          }
          right={
            <>
              <label className="flex items-center gap-1.5 text-[11px] text-muted">
                <input
                  type="checkbox"
                  checked={follow}
                  onChange={(event) => setFollow(event.target.checked)}
                  className="accent-current"
                />
                Follow
              </label>
              <label className="flex items-center gap-1.5 text-[11px] text-muted">
                <input
                  type="checkbox"
                  checked={wrap}
                  onChange={(event) => setWrap(event.target.checked)}
                  className="accent-current"
                />
                Wrap
              </label>
              <select
                value={lines}
                onChange={(event) => setLines(Number(event.target.value))}
                className="border border-border bg-bg px-2 py-1 text-[11px]"
              >
                <option value={100}>100 lines</option>
                <option value={200}>200 lines</option>
                <option value={500}>500 lines</option>
                <option value={1000}>1000 lines</option>
              </select>
            </>
          }
        />
        {filtersOpen && (
          <div className="flex flex-col gap-2 border-b border-border px-3 py-2 md:hidden">
            <select
              aria-label="Severity"
              value={severity}
              onChange={(event) => setSeverity(event.target.value as "all" | LogSeverity)}
              className="w-full border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value="all">All severities</option>
              <option value="error">Errors</option>
              <option value="warn">Warnings</option>
              <option value="info">Info</option>
            </select>
            <select
              aria-label="Source"
              value={source}
              onChange={(event) => setSource(event.target.value as "all" | LogSource)}
              className="w-full border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {sourceOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <select
              aria-label="Event type"
              value={eventType}
              onChange={(event) => setEventType(event.target.value as "all" | LogEventType)}
              className="w-full border border-border bg-bg px-2 py-1 text-[11px]"
            >
              {eventTypeOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.value === "all"
                    ? option.label
                    : `${option.label} (${eventCounts.get(option.value) ?? 0})`}
                </option>
              ))}
            </select>
          </div>
        )}
        <div className="max-h-[calc(100dvh-16rem)] overflow-y-auto overflow-x-auto p-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {filteredLines.length === 0 ? (
            <StateEmpty message={empty.noLogsMatch} />
          ) : (
            <div className="space-y-0.5">
              {filteredLines.map((line, index) => (
                <div
                  key={`${index}-${line.raw}`}
                  className={cn(
                    "border-l-2 px-2 py-0.5 text-[11px] leading-[1.55] text-[#e6edf3]",
                    wrap ? "whitespace-pre-wrap" : "whitespace-pre",
                    lineTone(line),
                  )}
                >
                  <Ansi>{line.raw}</Ansi>
                </div>
              ))}
              <div ref={bottomRef} />
            </div>
          )}
        </div>
    </Surface>
  );
}
