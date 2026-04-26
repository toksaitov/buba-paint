"""
Signal Frequency: Signals in context of price action.

Adapts presentation based on signal count:
- < 50 signals: Timeline view with BTC price and signal overlay markers
- >= 50 signals: Hour-of-day histogram + cumulative count

Always shows signal rate and per-strategy breakdown as on-chart annotations.
"""

import sqlite3
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

if len(sys.argv) != 2:
    print("Usage: python3 scripts/analysis/signal_frequency.py <db_path>")
    sys.exit(2)

DB_PATH = sys.argv[1]

conn = sqlite3.connect(DB_PATH)

sig_df = pd.read_sql_query(
    "SELECT timestamp, strategy, direction FROM signals ORDER BY timestamp", conn)

# Get BTC price for context
btc_df = pd.read_sql_query(
    "SELECT timestamp, price FROM tick_data "
    "WHERE source = 'binance' AND price IS NOT NULL ORDER BY timestamp", conn)

# Get markets for window shading
mf = pd.read_sql_query(
    "SELECT start_time, end_time FROM markets ORDER BY start_time", conn)

# Get total tick timespan for accurate runtime
tick_span = pd.read_sql_query(
    "SELECT MIN(timestamp) as t_min, MAX(timestamp) as t_max FROM tick_data", conn)

conn.close()

if sig_df.empty:
    print("No signals found. Run the bot longer to collect data.")
    fig, ax = plt.subplots(figsize=(12, 6))
    ax.text(0.5, 0.5, "No signals detected\nStrategies have not fired yet",
            transform=ax.transAxes, ha="center", va="center", fontsize=16, color="gray")
    ax.set_title("Signal Frequency")
    plt.savefig("signal_frequency.png", dpi=150)
    print("Saved placeholder to signal_frequency.png")
    sys.exit(0)

sig_df["time"] = pd.to_datetime(sig_df["timestamp"], unit="ms")

# Runtime from tick data (avoids 0-hour bug when all signals share same timestamp)
if not tick_span.empty and tick_span.iloc[0]["t_min"] is not None:
    runtime_ms = tick_span.iloc[0]["t_max"] - tick_span.iloc[0]["t_min"]
else:
    runtime_ms = sig_df["timestamp"].max() - sig_df["timestamp"].min()
runtime_hours = max(runtime_ms / 3_600_000, 0.001)

print(f"Total signals: {len(sig_df)}")
print(f"Runtime: {runtime_hours:.1f} hours")
for strategy in sig_df["strategy"].unique():
    sdf = sig_df[sig_df["strategy"] == strategy]
    print(f"\n{strategy}: {len(sdf)} signals ({len(sdf)/runtime_hours:.1f}/hr)")
    for direction in sdf["direction"].unique():
        count = len(sdf[sdf["direction"] == direction])
        print(f"  {direction}: {count}")

# Build stats annotation
strat_lines = []
for s in sig_df["strategy"].unique():
    n = len(sig_df[sig_df["strategy"] == s])
    strat_lines.append(f"  {s}: {n} ({n/runtime_hours:.1f}/hr)")
stats_text = (
    f"Total signals: {len(sig_df)}\n"
    f"Runtime: {runtime_hours:.1f} hours\n"
    + "\n".join(strat_lines)
)

# Marker styles per strategy
strategy_markers = {
    "latency-arb": ("^", "#E91E63", 60),     # pink triangles
    "spread-capture": ("s", "#FF9800", 50),   # orange squares
}
direction_offset = {"UP": 1, "DOWN": -1}

if len(sig_df) < 50:
    fig, ax = plt.subplots(figsize=(16, 8))

    if not btc_df.empty:
        btc_df["time"] = pd.to_datetime(btc_df["timestamp"], unit="ms")
        run_minutes = (btc_df["timestamp"].max() - btc_df["timestamp"].min()) / 60_000
        if run_minutes > 30:
            btc_plot = btc_df.set_index("time").resample("30s")["price"].last().dropna().reset_index()
        else:
            btc_plot = btc_df
        ax.plot(btc_plot["time"], btc_plot["price"], linewidth=0.8,
                alpha=0.7, color="#2196F3", label="BTC (Binance)")

    for i, (_, row) in enumerate(mf.iterrows()):
        s = pd.to_datetime(row["start_time"], unit="ms")
        e = pd.to_datetime(row["end_time"], unit="ms")
        if i % 2 == 0:
            ax.axvspan(s, e, alpha=0.06, color="gray")

    for strategy in sig_df["strategy"].unique():
        sdf = sig_df[sig_df["strategy"] == strategy]
        marker, color, size = strategy_markers.get(strategy, ("o", "gray", 40))

        for direction in sdf["direction"].unique():
            ddf = sdf[sdf["direction"] == direction]
            if not btc_df.empty:
                y_vals = []
                for _, sig_row in ddf.iterrows():
                    idx = (btc_df["timestamp"] - sig_row["timestamp"]).abs().idxmin()
                    y_vals.append(btc_df.loc[idx, "price"])
            else:
                y_vals = [0] * len(ddf)

            m = "^" if direction == "UP" else "v"
            ax.scatter(ddf["time"], y_vals, marker=m, color=color,
                       s=size, zorder=5, edgecolors="black", linewidth=0.5,
                       label=f"{strategy} {direction}")

    ax.set_xlabel("Time")
    ax.set_ylabel("BTC Price ($)")
    ax.set_title(f"Signal Events on BTC Price ({len(sig_df)} signals)")
    ax.legend(loc="upper left", fontsize=8)
    ax.grid(True, alpha=0.3)

    ax.text(0.98, 0.95, stats_text, transform=ax.transAxes,
            fontsize=9, verticalalignment="top", horizontalalignment="right",
            fontfamily="monospace",
            bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

else:
    sig_df["time_et"] = sig_df["time"].dt.tz_localize("UTC").dt.tz_convert("US/Eastern")
    sig_df["hour"] = sig_df["time_et"].dt.hour

    fig, axes = plt.subplots(1, 2, figsize=(14, 6))

    strategies = sig_df["strategy"].unique()
    hours = range(24)
    width = 0.35

    for i, strategy in enumerate(strategies):
        sdf = sig_df[sig_df["strategy"] == strategy]
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

    for strategy in strategies:
        sdf = sig_df[sig_df["strategy"] == strategy].sort_values("time")
        axes[1].plot(sdf["time"], range(1, len(sdf) + 1), label=strategy, linewidth=1.5)

    axes[1].set_xlabel("Time")
    axes[1].set_ylabel("Cumulative Signals")
    axes[1].set_title("Cumulative Signal Count Over Time")
    axes[1].legend()
    axes[1].grid(True, alpha=0.3)

    axes[1].text(0.02, 0.95, stats_text, transform=axes[1].transAxes,
                 fontsize=9, verticalalignment="top", fontfamily="monospace",
                 bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

plt.tight_layout()
plt.savefig("signal_frequency.png", dpi=150)
print("Saved to signal_frequency.png")
