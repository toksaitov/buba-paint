import { useMemo } from "react";
import { Button, StatusChip } from "../ui/dashboard-primitives";
import { downloadResearchReportCsvFromText } from "../../lib/research-api";

interface CsvPreviewProps {
  csv: string;
  maxRows?: number;
  filename?: string;
  downloadable?: boolean;
}

function parseCsv(csv: string): string[][] {
  const rows: string[][] = [];
  for (const line of csv.split(/\r?\n/)) {
    if (line === "" && rows.length > 0) continue;
    rows.push(line.split(","));
  }
  return rows;
}

export function CsvPreview({
  csv,
  maxRows = 20,
  filename = "report.csv",
  downloadable = false,
}: CsvPreviewProps) {
  const rows = useMemo(() => parseCsv(csv), [csv]);
  const truncated = rows.length > maxRows + 1;
  const visible = truncated ? rows.slice(0, maxRows + 1) : rows;

  if (rows.length === 0) {
    return (
      <div className="text-[12px] text-muted">CSV payload is empty.</div>
    );
  }

  const [header, ...data] = visible;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-[11px] text-muted">
          <span>{rows.length - 1} rows</span>
          {truncated && (
            <StatusChip
              label={`showing first ${maxRows}`}
              tone="muted"
              compact
            />
          )}
        </div>
        {downloadable && (
          <Button
            size="sm"
            onClick={() => downloadResearchReportCsvFromText(csv, filename)}
          >
            Download
          </Button>
        )}
      </div>
      <div className="overflow-x-auto border border-border">
        <table className="w-full border-collapse text-[11px] font-mono">
          <thead>
            <tr className="border-b border-border bg-surface">
              {header?.map((cell, idx) => (
                <th
                  key={idx}
                  className="px-2 py-1 text-left font-semibold text-text"
                >
                  {cell}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.map((row, ridx) => (
              <tr key={ridx} className="border-b border-border last:border-b-0">
                {row.map((cell, cidx) => (
                  <td key={cidx} className="px-2 py-1 tabular-nums">
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
