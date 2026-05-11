import { useEffect, useMemo, useRef, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { SlidersHorizontal } from "lucide-react";
import { Loading } from "../components/common/loading";
import { StateEmpty, Surface } from "../components/ui/dashboard-primitives";
import { useLogs } from "../hooks/use-logs";
import { empty } from "../lib/copy";
import { parseAnsi, stripAnsi } from "../lib/log-ansi";
import { cn, formatLogTimestamp } from "../lib/utils";

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

interface LogsPreferences {
  lines: number;
  follow: boolean;
  wrap: boolean;
  search: string;
  severity: "all" | LogSeverity;
  source: "all" | LogSource;
  eventType: "all" | LogEventType;
}

interface ParsedLogLine {
  raw: string;
  display: string;
  text: string;
  severity: LogSeverity;
  source: LogSource;
  sourceLabel: string;
  message: string;
  eventType: LogEventType;
}

const LEADING_RFC3339_UTC_PATTERN =
  /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)(.*)$/;
const LOGS_PREFS_STORAGE_KEY = "buba.logs.preferences.v1";
const DEFAULT_LOGS_PREFERENCES: LogsPreferences = {
  lines: 200,
  follow: true,
  wrap: false,
  search: "",
  severity: "all",
  source: "all",
  eventType: "all",
};

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

function isLineCount(value: unknown): value is LogsPreferences["lines"] {
  return value === 100 || value === 200 || value === 500 || value === 1000;
}

function isSeverity(value: unknown): value is LogsPreferences["severity"] {
  return value === "all" || value === "error" || value === "warn" || value === "info";
}

function isSource(value: unknown): value is LogsPreferences["source"] {
  return sourceOptions.some((option) => option.value === value) || value === "other";
}

function isEventType(value: unknown): value is LogsPreferences["eventType"] {
  return eventTypeOptions.some((option) => option.value === value) || value === "other";
}

function readLogsPreferences(): LogsPreferences {
  try {
    const raw = localStorage.getItem(LOGS_PREFS_STORAGE_KEY);
    if (!raw) return DEFAULT_LOGS_PREFERENCES;
    const parsed = JSON.parse(raw) as Partial<LogsPreferences>;
    return {
      lines: isLineCount(parsed.lines) ? parsed.lines : DEFAULT_LOGS_PREFERENCES.lines,
      follow:
        typeof parsed.follow === "boolean" ? parsed.follow : DEFAULT_LOGS_PREFERENCES.follow,
      wrap: typeof parsed.wrap === "boolean" ? parsed.wrap : DEFAULT_LOGS_PREFERENCES.wrap,
      search: typeof parsed.search === "string" ? parsed.search : DEFAULT_LOGS_PREFERENCES.search,
      severity: isSeverity(parsed.severity)
        ? parsed.severity
        : DEFAULT_LOGS_PREFERENCES.severity,
      source: isSource(parsed.source) ? parsed.source : DEFAULT_LOGS_PREFERENCES.source,
      eventType: isEventType(parsed.eventType)
        ? parsed.eventType
        : DEFAULT_LOGS_PREFERENCES.eventType,
    };
  } catch {
    return DEFAULT_LOGS_PREFERENCES;
  }
}

function lineSeverity(line: string): LogSeverity {
  if (line.includes(" ERROR ")) return "error";
  if (line.includes(" WARN ")) return "warn";
  if (line.includes(" INFO ")) return "info";
  return "other";
}

function localizeLogTimestamp(line: string): string {
  const match = line.match(LEADING_RFC3339_UTC_PATTERN);
  if (!match) return line;
  const epochMs = Date.parse(match[1]);
  if (!Number.isFinite(epochMs)) return line;
  return `${formatLogTimestamp(epochMs)}${match[2]}`;
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
  const display = localizeLogTimestamp(line);
  const parseText = stripAnsi(line);
  const text = stripAnsi(display);
  const severity = lineSeverity(parseText);
  const match = parseText.match(/\b(?:INFO|WARN|ERROR)\s+([A-Za-z0-9_:]+):?\s*(.*)$/);
  const sourceToken = match?.[1]?.replace(/:$/, "") ?? "unknown";
  const message = match?.[2]?.trim() ?? text;
  const source = classifySource(sourceToken, text);

  return {
    raw: line,
    display,
    text,
    severity,
    source,
    sourceLabel: sourceToken,
    message,
    eventType: classifyEventType(severity, source, message, text),
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
  const initialPreferences = useMemo(() => readLogsPreferences(), []);
  const [lines, setLines] = useState(initialPreferences.lines);
  const [follow, setFollow] = useState(initialPreferences.follow);
  const [search, setSearch] = useState(initialPreferences.search);
  const [severity, setSeverity] = useState<"all" | LogSeverity>(initialPreferences.severity);
  const [source, setSource] = useState<"all" | LogSource>(initialPreferences.source);
  const [eventType, setEventType] = useState<"all" | LogEventType>(
    initialPreferences.eventType,
  );
  const [wrap, setWrap] = useState(initialPreferences.wrap);
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
          !line.text.toLowerCase().includes(query) &&
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

  useEffect(() => {
    const preferences: LogsPreferences = {
      lines,
      follow,
      wrap,
      search,
      severity,
      source,
      eventType,
    };
    localStorage.setItem(LOGS_PREFS_STORAGE_KEY, JSON.stringify(preferences));
  }, [eventType, follow, lines, search, severity, source, wrap]);

  if (isLoading) return <Loading label="Loading logs" />;

  return (
    <div className="flex h-full flex-col">
      <Surface className="flex flex-1 flex-col overflow-hidden">
        <div className="grid gap-2 border-b border-border px-3 py-2 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
          <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-2 md:grid-cols-[minmax(0,1fr)_minmax(7rem,auto)_minmax(8rem,auto)_minmax(10rem,auto)]">
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search log lines"
              className="min-w-0 w-full border border-border bg-bg px-2 py-1 text-[11px]"
            />
            <button
              type="button"
              aria-label={
                activeFilterCount > 0
                  ? `Filters, ${activeFilterCount} active`
                  : "Filters"
              }
              aria-expanded={filtersOpen}
              title={
                activeFilterCount > 0
                  ? `Filters, ${activeFilterCount} active`
                  : "Filters"
              }
              onClick={() => setFiltersOpen((open) => !open)}
              className="inline-flex shrink-0 items-center gap-1 border border-border bg-bg px-2 py-1 text-[11px] md:hidden"
            >
              <SlidersHorizontal size={12} aria-hidden />
              {activeFilterCount > 0 && (
                <span className="tabular-nums">{activeFilterCount}</span>
              )}
            </button>
            <select
              aria-label="Severity"
              value={severity}
              onChange={(event) => setSeverity(event.target.value as "all" | LogSeverity)}
              className="hidden min-w-0 border border-border bg-bg px-2 py-1 text-[11px] md:block"
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
              className="hidden min-w-0 border border-border bg-bg px-2 py-1 text-[11px] md:block"
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
              className="hidden min-w-0 border border-border bg-bg px-2 py-1 text-[11px] md:block"
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
          <div className="flex min-w-0 flex-wrap items-center gap-2 lg:justify-end">
            <label className="inline-flex shrink-0 items-center text-[11px]">
              <input
                type="checkbox"
                checked={follow}
                onChange={(event) => setFollow(event.target.checked)}
                className="peer sr-only"
              />
              <span className="border border-border px-2 py-1 text-muted transition-colors peer-checked:border-text peer-checked:bg-text peer-checked:text-bg">
                Follow
              </span>
            </label>
            <label className="inline-flex shrink-0 items-center text-[11px]">
              <input
                type="checkbox"
                checked={wrap}
                onChange={(event) => setWrap(event.target.checked)}
                className="peer sr-only"
              />
              <span className="border border-border px-2 py-1 text-muted transition-colors peer-checked:border-text peer-checked:bg-text peer-checked:text-bg">
                Wrap
              </span>
            </label>
            <select
              aria-label="Line count"
              value={lines}
              onChange={(event) => setLines(Number(event.target.value))}
              className="min-w-[8rem] shrink-0 border border-border bg-bg px-2 py-1 text-[11px]"
            >
              <option value={100}>100 lines</option>
              <option value={200}>200 lines</option>
              <option value={500}>500 lines</option>
              <option value={1000}>1000 lines</option>
            </select>
          </div>
        </div>
        {filtersOpen && (
          <div className="grid gap-2 border-b border-border px-3 py-2 sm:grid-cols-3 md:hidden">
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
        <div className="min-h-[200px] flex-1 overflow-y-auto overflow-x-auto p-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {filteredLines.length === 0 ? (
            <StateEmpty message={empty.noLogsMatch} />
          ) : (
            <div className="space-y-0.5">
              {filteredLines.map((line, index) => (
                <div
                  key={`${index}-${line.raw}`}
                  className={cn(
                    "border-l-2 px-2 py-0.5 text-[11px] leading-[1.55] text-text",
                    wrap ? "whitespace-pre-wrap" : "whitespace-pre",
                    lineTone(line),
                  )}
                >
                  {parseAnsi(line.display).map((token, i) => (
                    <span key={i} className={token.className}>
                      {token.text}
                    </span>
                  ))}
                </div>
              ))}
              <div ref={bottomRef} />
            </div>
          )}
        </div>
    </Surface>
    </div>
  );
}
