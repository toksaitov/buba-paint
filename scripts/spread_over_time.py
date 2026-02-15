"""
Spread Over Time: Combined ask (UP + DOWN) analysis across 5-minute windows.

Uses EXACT timestamp matching (not cross-join) to avoid phantom sub-$1.00
readings from stale data.

Panel 1: Combined ask time series (resampled to 30s for readability).
Panel 2: Per-window market balance bar chart showing the cheaper side's
         average ask for each window.
"""

import sqlite3
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "data/buba-paint.db"

conn = sqlite3.connect(DB_PATH)

# EXACT timestamp join — fixes the 1500ms cross-join false positives
query = """
SELECT
    u.timestamp,
    u.ask AS up_ask,
    d.ask AS down_ask,
    u.ask + d.ask AS total_ask
FROM tick_data u
JOIN tick_data d
    ON d.source = 'clob_down'
    AND d.timestamp = u.timestamp
WHERE u.source = 'clob_up'
    AND u.ask IS NOT NULL AND u.ask > 0
    AND d.ask IS NOT NULL AND d.ask > 0
ORDER BY u.timestamp
"""

mf = pd.read_sql_query(
    "SELECT start_time, end_time, question, market_id FROM markets ORDER BY start_time", conn)

df = pd.read_sql_query(query, conn)
conn.close()

if df.empty:
    print("No CLOB spread data found (exact timestamp match). Run the bot longer.")
    sys.exit(0)

df["time"] = pd.to_datetime(df["timestamp"], unit="ms")

# Stats
below_1 = df[df["total_ask"] < 1.0]
below_0998 = df[df["total_ask"] < 0.998]
mean_total = df["total_ask"].mean()
min_total = df["total_ask"].min()

print(f"Total spread samples (exact match): {len(df)}")
print(f"Mean total ask: {mean_total:.4f}")
print(f"Min total ask: {min_total:.4f}")
print(f"Samples below $1.000: {len(below_1)} ({100*len(below_1)/len(df):.1f}%)")
print(f"Samples below $0.998: {len(below_0998)} ({100*len(below_0998)/len(df):.1f}%)")

# Resample to 30s if run > 30 min
run_minutes = (df["timestamp"].max() - df["timestamp"].min()) / 60_000
df_plot = df.copy()
if run_minutes > 30:
    df_plot = df_plot.set_index("time").resample("30s").last().dropna().reset_index()

# --- Per-window market balance ---
# For each window, compute the average of the cheaper side's ask
window_stats = []
for _, row in mf.iterrows():
    mask = (df["timestamp"] >= row["start_time"]) & (df["timestamp"] < row["end_time"])
    wdf = df[mask]
    if wdf.empty:
        continue
    avg_up = wdf["up_ask"].mean()
    avg_down = wdf["down_ask"].mean()
    cheaper_side = "UP" if avg_up <= avg_down else "DOWN"
    cheaper_ask = min(avg_up, avg_down)
    window_stats.append({
        "start": pd.to_datetime(row["start_time"], unit="ms"),
        "cheaper_side": cheaper_side,
        "cheaper_ask": cheaper_ask,
        "total_ask": wdf["total_ask"].mean(),
        "samples": len(wdf),
    })

wdf_plot = pd.DataFrame(window_stats)

# --- Plot ---
fig, axes = plt.subplots(2, 1, figsize=(16, 11))

# Panel 1: Combined ask time series
# Alternating window shading
for i, (_, row) in enumerate(mf.iterrows()):
    s = pd.to_datetime(row["start_time"], unit="ms")
    e = pd.to_datetime(row["end_time"], unit="ms")
    if i % 2 == 0:
        axes[0].axvspan(s, e, alpha=0.06, color="gray")

axes[0].plot(df_plot["time"], df_plot["total_ask"], linewidth=0.8,
             alpha=0.9, color="#2196F3")
axes[0].axhline(1.0, color="red", linestyle="--", linewidth=1,
                label="$1.00 (fair value)")
axes[0].axhline(0.998, color="orange", linestyle="--", linewidth=1,
                label="$0.998 (threshold)")
axes[0].set_ylabel("UP Ask + DOWN Ask ($)")
axes[0].set_title("Combined Ask Price Over Time (30s resample, exact timestamp match)")
axes[0].legend(loc="upper left")
axes[0].grid(True, alpha=0.3)

# Stats text box
stats_text = (
    f"Samples: {len(df):,}\n"
    f"Mean total ask: ${mean_total:.4f}\n"
    f"Min total ask: ${min_total:.4f}\n"
    f"Below $1.000: {len(below_1)} ({100*len(below_1)/max(1,len(df)):.1f}%)\n"
    f"Below $0.998: {len(below_0998)} ({100*len(below_0998)/max(1,len(df)):.1f}%)"
)
axes[0].text(0.02, 0.95, stats_text, transform=axes[0].transAxes,
             fontsize=9, verticalalignment="top", fontfamily="monospace",
             bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

# Panel 2: Per-window market balance bar chart
if not wdf_plot.empty:
    colors = ["#4CAF50" if s == "UP" else "#F44336" for s in wdf_plot["cheaper_side"]]
    bar_positions = range(len(wdf_plot))
    axes[1].bar(bar_positions, wdf_plot["cheaper_ask"], color=colors, alpha=0.8,
                edgecolor="black", linewidth=0.3)
    axes[1].axhline(0.5, color="gray", linestyle=":", alpha=0.5, label="50/50 balance")

    # Add legend for colors
    from matplotlib.patches import Patch
    legend_elements = [
        Patch(facecolor="#4CAF50", label="UP cheaper"),
        Patch(facecolor="#F44336", label="DOWN cheaper"),
    ]
    axes[1].legend(handles=legend_elements, loc="upper right")

    # X-axis: show time labels for a subset of windows
    n_windows = len(wdf_plot)
    if n_windows <= 20:
        axes[1].set_xticks(bar_positions)
        axes[1].set_xticklabels(
            [t.strftime("%H:%M") for t in wdf_plot["start"]], rotation=45, fontsize=7)
    else:
        step = max(1, n_windows // 15)
        ticks = list(range(0, n_windows, step))
        axes[1].set_xticks(ticks)
        axes[1].set_xticklabels(
            [wdf_plot.iloc[i]["start"].strftime("%H:%M") for i in ticks],
            rotation=45, fontsize=7)

    axes[1].set_xlabel("Window Start Time")
    axes[1].set_ylabel("Cheaper Side Avg Ask ($)")
    axes[1].set_title(f"Per-Window Market Balance ({n_windows} windows)")
    axes[1].grid(True, alpha=0.3, axis="y")
else:
    axes[1].text(0.5, 0.5, "No window data available", transform=axes[1].transAxes,
                 ha="center", va="center", fontsize=14, color="gray")

plt.tight_layout()
plt.savefig("spread_over_time.png", dpi=150)
print("Saved to spread_over_time.png")
