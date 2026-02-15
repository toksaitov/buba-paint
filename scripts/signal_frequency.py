"""
Signal Frequency: How many opportunities per hour does each strategy detect,
broken down by time of day (Eastern Time).
"""

import sqlite3
import sys

import matplotlib.pyplot as plt
import pandas as pd

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "data/buba-paint.db"

conn = sqlite3.connect(DB_PATH)

query = "SELECT timestamp, strategy, direction FROM signals ORDER BY timestamp"
df = pd.read_sql_query(query, conn)
conn.close()

if df.empty:
    print("No signals found. Run the bot longer to collect data.")
    sys.exit(0)

df["time"] = pd.to_datetime(df["timestamp"], unit="ms")
# Convert to US Eastern
df["time_et"] = df["time"].dt.tz_localize("UTC").dt.tz_convert("US/Eastern")
df["hour"] = df["time_et"].dt.hour

print(f"Total signals: {len(df)}")
for strategy in df["strategy"].unique():
    sdf = df[df["strategy"] == strategy]
    print(f"\n{strategy}: {len(sdf)} signals")
    for direction in sdf["direction"].unique():
        count = len(sdf[sdf["direction"] == direction])
        print(f"  {direction}: {count}")

# Calculate runtime to normalize to signals per hour
runtime_hours = (df["timestamp"].max() - df["timestamp"].min()) / 3_600_000
if runtime_hours > 0:
    print(f"\nRuntime: {runtime_hours:.1f} hours")
    for strategy in df["strategy"].unique():
        count = len(df[df["strategy"] == strategy])
        print(f"  {strategy}: {count/runtime_hours:.1f} signals/hour")

fig, axes = plt.subplots(1, 2, figsize=(14, 5))

# Grouped bar chart: signals per hour-of-day by strategy
strategies = df["strategy"].unique()
hours = range(24)
width = 0.35

for i, strategy in enumerate(strategies):
    sdf = df[df["strategy"] == strategy]
    counts = sdf.groupby("hour").size().reindex(hours, fill_value=0)
    offset = (i - len(strategies) / 2 + 0.5) * width
    axes[0].bar([h + offset for h in hours], counts, width=width,
                label=strategy, alpha=0.8)

axes[0].set_xlabel("Hour of Day (ET)")
axes[0].set_ylabel("Signal Count")
axes[0].set_title("Signals by Hour of Day")
axes[0].set_xticks(range(0, 24, 2))
axes[0].legend()
axes[0].grid(True, alpha=0.3, axis="y")

# Cumulative signals over time
for strategy in strategies:
    sdf = df[df["strategy"] == strategy].sort_values("time")
    axes[1].plot(sdf["time"], range(1, len(sdf) + 1), label=strategy, linewidth=1.5)

axes[1].set_xlabel("Time")
axes[1].set_ylabel("Cumulative Signals")
axes[1].set_title("Cumulative Signal Count Over Time")
axes[1].legend()
axes[1].grid(True, alpha=0.3)

plt.tight_layout()
plt.savefig("signal_frequency.png", dpi=150)
plt.show()
print("Saved to signal_frequency.png")
