import { useMemo } from "react";
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useTheme } from "../../hooks/use-theme";
import { getChartColors } from "../../lib/chart-colors";
import { formatChartTick, formatChartTime } from "../../lib/utils";

export interface TimelineSeries {
  label: string;
  dataKey: string;
  values: { ts_ms: number; value: number }[];
  color: string;
  emphasize?: boolean;
  yAxisFormat?: (v: number) => string;
}

export interface MachineTimelineProps {
  series: TimelineSeries[];
  yMin?: number;
  yMax?: number;
  height?: number;
  ariaLabel?: string;
}

interface TooltipPayloadItem {
  name?: string;
  value?: number;
  color?: string;
}

function mergeSeriesByTime(
  series: TimelineSeries[],
): Array<Record<string, number>> {
  const rows = new Map<number, Record<string, number>>();
  for (const s of series) {
    for (const point of s.values) {
      const row = rows.get(point.ts_ms) ?? { ts_ms: point.ts_ms };
      row[s.dataKey] = point.value;
      rows.set(point.ts_ms, row);
    }
  }
  return Array.from(rows.values()).sort((a, b) => a.ts_ms - b.ts_ms);
}

export function MachineTimeline({
  series,
  yMin,
  yMax,
  height = 160,
  ariaLabel,
}: MachineTimelineProps) {
  const { theme } = useTheme();
  const colors = getChartColors(theme);

  const merged = useMemo(() => mergeSeriesByTime(series), [series]);
  const yTickFormatter = series[0]?.yAxisFormat ?? ((v: number) => String(v));

  return (
    <div
      role="img"
      aria-label={ariaLabel}
      style={{ width: "100%", height }}
      className="text-[10px]"
    >
      <ResponsiveContainer>
        <ComposedChart
          data={merged}
          margin={{ top: 4, right: 8, bottom: 4, left: 0 }}
        >
          <CartesianGrid stroke={colors.gridColor} strokeDasharray="3 3" />
          <XAxis
            dataKey="ts_ms"
            type="number"
            domain={["dataMin", "dataMax"]}
            tickFormatter={(v: number) => formatChartTick(v)}
            stroke={colors.textColor}
            tick={{ fontSize: 10, fontFamily: "ui-monospace" }}
          />
          <YAxis
            domain={[yMin ?? "auto", yMax ?? "auto"]}
            tickFormatter={(v: number) => yTickFormatter(v)}
            stroke={colors.textColor}
            tick={{ fontSize: 10, fontFamily: "ui-monospace" }}
            width={48}
          />
          <Tooltip
            labelFormatter={(v) => formatChartTime(Number(v))}
            contentStyle={{
              background: colors.tooltipBg,
              border: `1px solid ${colors.borderColor}`,
              fontSize: 11,
              fontFamily: "ui-monospace",
              color: colors.textColor,
            }}
            itemStyle={{ color: colors.textColor }}
            formatter={(value, name) => {
              const v = typeof value === "number" ? value : Number(value);
              return [
                Number.isFinite(v) ? yTickFormatter(v) : String(value),
                String(name),
              ];
            }}
          />
          {series.map((s) =>
            s.emphasize ? (
              <Area
                key={s.dataKey}
                type="monotone"
                dataKey={s.dataKey}
                name={s.label}
                stroke={s.color}
                fill={s.color}
                fillOpacity={0.18}
                strokeWidth={2}
                isAnimationActive={false}
                connectNulls
              />
            ) : (
              <Line
                key={s.dataKey}
                type="monotone"
                dataKey={s.dataKey}
                name={s.label}
                stroke={s.color}
                strokeWidth={1}
                dot={false}
                isAnimationActive={false}
                connectNulls
              />
            ),
          )}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

export type { TooltipPayloadItem };
