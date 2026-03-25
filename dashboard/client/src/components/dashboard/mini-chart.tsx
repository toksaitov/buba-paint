import { useRef, useEffect } from "react";
import { createChart, AreaSeries, type IChartApi } from "lightweight-charts";
import type { BalanceEntry } from "../../lib/types";

export function MiniChart({ entries }: { entries: BalanceEntry[] }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const chart = createChart(containerRef.current, {
      height: 120,
      layout: {
        background: { color: "#ffffff" },
        textColor: "#656d76",
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
        fontSize: 10,
        attributionLogo: false,
      },
      grid: {
        vertLines: { visible: false },
        horzLines: { color: "#f6f8fa" },
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
      lineColor: "#1f2328",
      lineWidth: 2,
      topColor: "rgba(31, 35, 40, 0.12)",
      bottomColor: "rgba(31, 35, 40, 0.02)",
      crosshairMarkerVisible: false,
    });

    // Deduplicate by timestamp — lightweight-charts requires strictly ascending times
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
  }, [entries]);

  return (
    <div className="border border-border px-3 py-2.5 bg-bg">
      <div className="text-[10px] uppercase tracking-wide text-muted mb-1.5">
        Equity Curve
      </div>
      <div ref={containerRef} />
    </div>
  );
}
