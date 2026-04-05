import { useRef, useEffect } from "react";
import { createChart, AreaSeries, type IChartApi } from "lightweight-charts";
import { useTheme } from "../../hooks/use-theme";
import { getChartColors } from "../../lib/chart-colors";
import type { BalanceEntry } from "../../lib/types";

export function MiniChart({ entries }: { entries: BalanceEntry[] }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const { isDark } = useTheme();

  useEffect(() => {
    if (!containerRef.current) return;

    const c = getChartColors(isDark);
    const chart = createChart(containerRef.current, {
      height: 120,
      layout: {
        background: { color: c.background },
        textColor: c.textColor,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
        fontSize: 10,
        attributionLogo: false,
      },
      grid: {
        vertLines: { visible: false },
        horzLines: { color: c.gridColor },
      },
      rightPriceScale: {
        borderVisible: false,
      },
      timeScale: {
        borderVisible: false,
        visible: false,
      },
      handleScroll: false,
      handleScale: false,
      crosshair: {
        vertLine: { visible: false },
        horzLine: { visible: false },
      },
    });

    const series = chart.addSeries(AreaSeries, {
      lineColor: c.lineColor,
      lineWidth: 2,
      topColor: c.areaTopColor,
      bottomColor: c.areaBottomColor,
      crosshairMarkerVisible: false,
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
  }, [entries, isDark]);

  return (
    <div className="border border-border px-3 py-2.5 bg-bg">
      <div className="text-[10px] uppercase tracking-wide text-muted mb-1.5">
        Equity Curve
      </div>
      <div ref={containerRef} />
    </div>
  );
}
