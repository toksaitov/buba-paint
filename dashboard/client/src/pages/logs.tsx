import { useRef, useEffect, useState } from "react";
import { useOutletContext } from "react-router-dom";
import AnsiModule from "ansi-to-react";
import { useLogs } from "../hooks/use-logs";
import { Loading } from "../components/common/loading";

// ansi-to-react ships CJS — Vite may double-wrap the default export
// Unwrap until we find the actual React component function
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _ansi: any = AnsiModule;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
while (typeof _ansi !== "function" && _ansi && typeof _ansi.default !== "undefined")
  _ansi = _ansi.default;
const Ansi: React.ComponentType<{ children: string }> = _ansi;

export function LogsPage() {
  const { botId } = useOutletContext<{ botId: string }>();
  const [lines, setLines] = useState(200);
  const { data, isLoading } = useLogs(botId, lines);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [data, autoScroll]);

  if (isLoading) return <Loading />;

  const logLines = data?.lines ?? [];

  return (
    <div className="space-y-3 flex flex-col h-full">
      <div className="flex items-center justify-between">
        <h2 className="text-[14px] font-bold">Bot Log</h2>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-[11px] text-muted">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
              className="accent-current"
            />
            Auto-scroll
          </label>
          <select
            value={lines}
            onChange={(e) => setLines(Number(e.target.value))}
            className="text-[11px] border border-border bg-bg px-2 py-1"
          >
            <option value={100}>100 lines</option>
            <option value={200}>200 lines</option>
            <option value={500}>500 lines</option>
            <option value={1000}>1000 lines</option>
          </select>
        </div>
      </div>
      <div className="border border-border bg-[#1f2328] flex-1 min-h-0 overflow-y-auto overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden p-3">
        <pre className="text-[11px] leading-[1.6] text-[#e6edf3] whitespace-pre">
          {logLines.length > 0 ? (
            <Ansi>{logLines.join("\n")}</Ansi>
          ) : (
            "No log output available."
          )}
        </pre>
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
