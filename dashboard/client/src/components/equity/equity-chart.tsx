import { useRef, useEffect } from "react";
import { createChart, AreaSeries, type IChartApi } from "lightweight-charts";
import { useMediaQuery } from "../../hooks/use-media-query";
import { useTheme } from "../../hooks/use-theme";
import { getChartColors } from "../../lib/chart-colors";
import type { BalanceEntry } from "../../lib/types";

export function EquityChart({ entries }: { entries: BalanceEntry[] }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const { isDark } = useTheme();
  const chartHeight = isDesktop ? 480 : 320;

  useEffect(() => {
    if (!containerRef.current) return;

    const c = getChartColors(isDark);
    const chart = createChart(containerRef.current, {
      height: chartHeight,
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
        chart.applyOptions({ width: containerRef.current.clientWidth });
      }
    });
    ro.observe(containerRef.current);

    return () => {
      ro.disconnect();
      chart.remove();
    };
  }, [entries, chartHeight, isDark]);

  return <div ref={containerRef} className="w-full" />;
}
