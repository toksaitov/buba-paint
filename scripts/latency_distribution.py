"""
Latency Distribution: Binance -> CLOB reaction delay histogram, CDF,
and CLOB update frequency diagnostic.

Includes annotation about 1-second tick sampling artifact causing bimodal
distribution, and a CLOB frequency panel for continuity diagnostics.
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

# Binance -> CLOB latency
latency_query = """
WITH binance_ticks AS (
    SELECT timestamp AS bts, price AS bprice,
           price - LAG(price) OVER (ORDER BY timestamp) AS price_delta
    FROM tick_data
    WHERE source = 'binance'
),
significant_moves AS (
    SELECT bts, bprice, price_delta
    FROM binance_ticks
    WHERE ABS(price_delta) > 0
),
clob_ticks AS (
    SELECT timestamp AS cts, bid, ask
    FROM tick_data
    WHERE source IN ('clob_up', 'clob_down')
)
SELECT
    s.bts,
    MIN(c.cts) AS first_clob_update,
    MIN(c.cts) - s.bts AS delay_ms,
    s.price_delta
FROM significant_moves s
JOIN clob_ticks c ON c.cts > s.bts AND c.cts <= s.bts + 15000
GROUP BY s.bts
HAVING delay_ms > 0
ORDER BY s.bts
"""

# CLOB update frequency
clob_freq_query = """
SELECT timestamp
FROM tick_data
WHERE source IN ('clob_up', 'clob_down')
ORDER BY timestamp
"""

df = pd.read_sql_query(latency_query, conn)
clob_df = pd.read_sql_query(clob_freq_query, conn)

# Markets for shading
mf = pd.read_sql_query(
    "SELECT start_time, end_time FROM markets ORDER BY start_time", conn)

conn.close()

if df.empty:
    print("No latency data found. Run the bot longer to collect more data.")
    sys.exit(0)

# Stats
mean_delay = df["delay_ms"].mean()
median_delay = df["delay_ms"].median()
p95 = df["delay_ms"].quantile(0.95)
p99 = df["delay_ms"].quantile(0.99)
n_samples = len(df)

print(f"Total latency samples: {n_samples}")
print(f"Mean delay: {mean_delay:.0f} ms")
print(f"Median delay: {median_delay:.0f} ms")
print(f"P95 delay: {p95:.0f} ms")
print(f"P99 delay: {p99:.0f} ms")

stats_text = (
    f"Samples: {n_samples:,}\n"
    f"Mean: {mean_delay:.0f} ms\n"
    f"Median: {median_delay:.0f} ms\n"
    f"P95: {p95:.0f} ms\n"
    f"P99: {p99:.0f} ms"
)

# --- Plot ---
fig, axes = plt.subplots(3, 1, figsize=(16, 14))

# Panel 1: Histogram
axes[0].hist(df["delay_ms"], bins=50, edgecolor="black", alpha=0.7, color="#2196F3")
axes[0].axvline(median_delay, color="red", linestyle="--",
                label=f"Median: {median_delay:.0f} ms")
axes[0].axvline(mean_delay, color="orange", linestyle="--",
                label=f"Mean: {mean_delay:.0f} ms")
axes[0].set_xlabel("Delay (ms)")
axes[0].set_ylabel("Frequency")
axes[0].set_title("Binance -> CLOB Reaction Latency Distribution")
axes[0].legend(loc="upper right")

# Stats text box
axes[0].text(0.02, 0.95, stats_text, transform=axes[0].transAxes,
             fontsize=9, verticalalignment="top", fontfamily="monospace",
             bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

# Sampling caveat annotation
caveat = (
    "NOTE: Bimodal pattern (0ms / 1000ms peaks)\n"
    "is an artifact of 1-second tick sampling.\n"
    "True sub-second latency requires event-\n"
    "level timestamps from WebSocket messages."
)
axes[0].text(0.98, 0.95, caveat, transform=axes[0].transAxes,
             fontsize=8, verticalalignment="top", horizontalalignment="right",
             fontfamily="monospace", fontstyle="italic", color="#666",
             bbox=dict(boxstyle="round,pad=0.4", facecolor="#f0f0f0", alpha=0.8))

# Panel 2: CDF
sorted_delays = df["delay_ms"].sort_values().values
cdf_norm = np.arange(1, len(sorted_delays) + 1) / len(sorted_delays)
axes[1].plot(sorted_delays, cdf_norm, color="#4CAF50", linewidth=1.5)
axes[1].axhline(0.5, color="gray", linestyle=":", alpha=0.5, label="50th percentile")
axes[1].axhline(0.95, color="gray", linestyle=":", alpha=0.5, label="95th percentile")
axes[1].axvline(median_delay, color="red", linestyle="--", alpha=0.5)
axes[1].axvline(p95, color="orange", linestyle="--", alpha=0.5)
axes[1].set_xlabel("Delay (ms)")
axes[1].set_ylabel("Cumulative Probability")
axes[1].set_title("Latency CDF")
axes[1].legend(loc="lower right")
axes[1].grid(True, alpha=0.3)

# Panel 3: CLOB update frequency over time
if not clob_df.empty:
    clob_df["time"] = pd.to_datetime(clob_df["timestamp"], unit="ms")
    # Count CLOB ticks per minute
    clob_per_min = clob_df.set_index("time").resample("1min").size().reset_index(name="count")

    # Window shading
    for i, (_, row) in enumerate(mf.iterrows()):
        s = pd.to_datetime(row["start_time"], unit="ms")
        e = pd.to_datetime(row["end_time"], unit="ms")
        if i % 2 == 0:
            axes[2].axvspan(s, e, alpha=0.06, color="gray")

    axes[2].plot(clob_per_min["time"], clob_per_min["count"],
                 linewidth=0.8, color="#FF5722", alpha=0.8)
    axes[2].set_xlabel("Time")
    axes[2].set_ylabel("CLOB Ticks / Minute")
    axes[2].set_title("CLOB Update Frequency (continuity diagnostic)")
    axes[2].grid(True, alpha=0.3)

    # Annotate stats
    avg_freq = clob_per_min["count"].mean()
    min_freq = clob_per_min["count"].min()
    gaps = clob_per_min[clob_per_min["count"] == 0]
    freq_text = (
        f"Avg: {avg_freq:.1f} ticks/min\n"
        f"Min: {min_freq} ticks/min\n"
        f"Zero-tick minutes: {len(gaps)}"
    )
    axes[2].text(0.02, 0.95, freq_text, transform=axes[2].transAxes,
                 fontsize=9, verticalalignment="top", fontfamily="monospace",
                 bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))
else:
    axes[2].text(0.5, 0.5, "No CLOB tick data available", transform=axes[2].transAxes,
                 ha="center", va="center", fontsize=14, color="gray")

plt.tight_layout()
plt.savefig("latency_distribution.png", dpi=150)
print("Saved to latency_distribution.png")
