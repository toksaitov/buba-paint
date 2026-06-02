import { Pie, PieChart, ResponsiveContainer } from "recharts";
import { useTheme } from "../../hooks/use-theme";
import { getChartColors } from "../../lib/chart-colors";

export type DonutTone = "default" | "warning" | "danger";

export interface DonutGaugeProps {
  used: number;
  total: number;
  label?: string;
  size?: number;
  tone?: DonutTone;
  ariaLabel?: string;
}

export function DonutGauge({
  used,
  total,
  label,
  size = 64,
  tone = "default",
  ariaLabel,
}: DonutGaugeProps) {
  const { theme } = useTheme();
  const colors = getChartColors(theme);
  const safeTotal = total > 0 ? total : 0;
  const safeUsed = Math.max(0, Math.min(used, safeTotal));
  const free = Math.max(0, safeTotal - safeUsed);
  const percent = safeTotal > 0 ? (safeUsed / safeTotal) * 100 : null;
  const usedColor =
    tone === "danger"
      ? colors.dangerColor
      : tone === "warning"
        ? colors.warningColor
        : colors.lineColor;

  const data =
    safeTotal > 0
      ? [
          { name: "used", value: safeUsed, fill: usedColor },
          { name: "free", value: free, fill: colors.mutedColor },
        ]
      : [{ name: "empty", value: 1, fill: colors.mutedColor }];

  return (
    <div
      role="img"
      aria-label={ariaLabel ?? label ?? "donut gauge"}
      className="flex flex-col items-center justify-center text-[10px]"
      style={{ width: size, height: size + (label ? 14 : 0) }}
    >
      <div style={{ width: size, height: size, position: "relative" }}>
        <ResponsiveContainer width={size} height={size}>
          <PieChart>
            <Pie
              data={data}
              dataKey="value"
              startAngle={90}
              endAngle={-270}
              innerRadius={size * 0.32}
              outerRadius={size * 0.46}
              stroke="none"
              isAnimationActive={false}
            />
          </PieChart>
        </ResponsiveContainer>
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: colors.textColor,
            fontFamily: "ui-monospace",
            fontSize: Math.max(10, size * 0.18),
            pointerEvents: "none",
          }}
        >
          {percent == null ? "-" : `${percent.toFixed(0)}%`}
        </div>
      </div>
      {label ? (
        <span
          className="mt-0.5 uppercase tracking-wide"
          style={{ color: colors.textColor }}
        >
          {label}
        </span>
      ) : null}
    </div>
  );
}
