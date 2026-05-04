import { useRef, useEffect } from "react";
import { createChart, AreaSeries, type IChartApi, type Time } from "lightweight-charts";
import { useTheme } from "../../hooks/use-theme";
import { getChartColors } from "../../lib/chart-colors";
import type { BalanceEntry } from "../../lib/types";
import { formatChartTick, formatChartTime } from "../../lib/utils";

function chartTimeToMs(time: Time): number {
  if (typeof time === "number") return time * 1000;
  if (typeof time === "string") return Date.parse(time);
  return new Date(time.year, time.month - 1, time.day).getTime();
}

export function EquityChart({ entries }: { entries: BalanceEntry[] }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const { theme } = useTheme();

  useEffect(() => {
    if (!containerRef.current) return;

    const c = getChartColors(theme);
    const chart = createChart(containerRef.current, {
      width: containerRef.current.clientWidth,
      height: containerRef.current.clientHeight,
      layout: {
        background: { color: c.background },
        textColor: c.textColor,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
        fontSize: 11,
        attributionLogo: false,
      },
      grid: {
        vertLines: { color: c.gridColor },
        horzLines: { color: c.gridColor },
      },
      rightPriceScale: {
        borderColor: c.borderColor,
      },
      timeScale: {
        borderColor: c.borderColor,
        timeVisible: true,
        secondsVisible: false,
        tickMarkFormatter: (time: Time) => formatChartTick(chartTimeToMs(time)),
      },
      localization: {
        timeFormatter: (time: Time) => formatChartTime(chartTimeToMs(time)),
      },
      crosshair: {
        vertLine: { color: c.crosshairColor, width: 1, style: 2 },
        horzLine: { color: c.crosshairColor, width: 1, style: 2 },
      },
    });

    const series = chart.addSeries(AreaSeries, {
      lineColor: c.lineColor,
      lineWidth: 2,
      topColor: c.areaTopColor,
      bottomColor: c.areaBottomColor,
    });

    const byTime = new Map<number, number>();
    for (const e of entries) {
      if (e.timestamp <= 0) continue;
      const t = Math.round(e.timestamp / 1000);
      byTime.set(t, e.balance);
    }
    const data = Array.from(byTime, ([time, value]) => ({
      time: time as import("lightweight-charts").UTCTimestamp,
      value,
    })).sort((a, b) => a.time - b.time);

    if (data.length > 0) {
      series.setData(data);
      chart.timeScale().fitContent();
    }

    chartRef.current = chart;

    const ro = new ResizeObserver(() => {
      if (containerRef.current) {
        chart.applyOptions({
          width: containerRef.current.clientWidth,
          height: containerRef.current.clientHeight,
        });
      }
    });
    ro.observe(containerRef.current);

    return () => {
      ro.disconnect();
      chart.remove();
    };
  }, [entries, theme]);

  return <div ref={containerRef} className="h-full w-full min-h-[200px]" />;
}
