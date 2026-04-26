"""
Binance vs Chainlink Delta: Time series of the price difference between
the two feeds, to understand oracle lag near window boundaries.

Designed for readability at multi-hour timescales with 30-second resampling,
alternating 5-minute window shading, and on-chart statistical annotations.
"""

import sqlite3
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

if len(sys.argv) != 2:
    print("Usage: python3 scripts/analysis/binance_vs_chainlink.py <db_path>")
    sys.exit(2)

DB_PATH = sys.argv[1]

conn = sqlite3.connect(DB_PATH)

bf = pd.read_sql_query(
    "SELECT timestamp, price AS binance_price FROM tick_data "
    "WHERE source = 'binance' AND price IS NOT NULL ORDER BY timestamp", conn)
cf = pd.read_sql_query(
    "SELECT timestamp, price AS chainlink_price FROM tick_data "
    "WHERE source = 'chainlink' AND price IS NOT NULL ORDER BY timestamp", conn)
mf = pd.read_sql_query(
    "SELECT start_time, end_time, question FROM markets ORDER BY start_time", conn)
conn.close()

if bf.empty or cf.empty:
    print("Insufficient data. Need both Binance and Chainlink ticks.")
    sys.exit(0)

bf["second"] = (bf["timestamp"] // 1000) * 1000
cf["second"] = (cf["timestamp"] // 1000) * 1000

bf_agg = bf.groupby("second")["binance_price"].last().reset_index()
cf_agg = cf.groupby("second")["chainlink_price"].last().reset_index()

merged = pd.merge(bf_agg, cf_agg, on="second", how="inner")
merged["time"] = pd.to_datetime(merged["second"], unit="ms")
merged["delta"] = merged["binance_price"] - merged["chainlink_price"]
merged["delta_pct"] = (merged["delta"] / merged["chainlink_price"]) * 100

run_minutes = (merged["second"].max() - merged["second"].min()) / 60_000
if run_minutes > 30:
    merged = merged.set_index("time").resample("30s").last().dropna().reset_index()

mean_d = merged["delta"].mean()
std_d = merged["delta"].std()
max_d = merged["delta"].max()
min_d = merged["delta"].min()
median_d = merged["delta"].median()

print(f"Merged samples: {len(merged)}")
print(f"Mean delta: ${mean_d:.2f} ({merged['delta_pct'].mean():.4f}%)")
print(f"Std delta: ${std_d:.2f}")
print(f"Max delta: ${max_d:.2f}")
print(f"Min delta: ${min_d:.2f}")

fig, axes = plt.subplots(3, 1, figsize=(16, 14))

def shade_windows(ax, markets_df):
    for i, (_, row) in enumerate(markets_df.iterrows()):
        s = pd.to_datetime(row["start_time"], unit="ms")
        e = pd.to_datetime(row["end_time"], unit="ms")
        if i % 2 == 0:
            ax.axvspan(s, e, alpha=0.06, color="gray")

# Panel 1: Price comparison
shade_windows(axes[0], mf)
axes[0].plot(merged["time"], merged["binance_price"], linewidth=0.8,
             alpha=0.9, color="#2196F3", label="Binance")
axes[0].plot(merged["time"], merged["chainlink_price"], linewidth=0.8,
             alpha=0.9, color="#FF9800", label="Chainlink")
axes[0].set_ylabel("BTC Price ($)")
axes[0].set_title("Binance vs Chainlink BTC Price (30s resample)")
axes[0].legend(loc="upper left")
axes[0].grid(True, alpha=0.3)

# Panel 2: Delta with rolling mean and sigma bands
shade_windows(axes[1], mf)
# 1-minute rolling mean (2 samples at 30s)
roll_window = max(2, int(60 / 30))
rolling_mean = merged["delta"].rolling(roll_window, min_periods=1).mean()
axes[1].plot(merged["time"], rolling_mean, linewidth=1.2, color="#9C27B0",
             label="1-min rolling mean")
axes[1].axhline(mean_d, color="blue", linestyle="--", linewidth=0.8,
                label=f"Mean: ${mean_d:.1f}", alpha=0.7)
axes[1].axhline(mean_d + std_d, color="orange", linestyle=":", linewidth=0.7, alpha=0.6)
axes[1].axhline(mean_d - std_d, color="orange", linestyle=":", linewidth=0.7,
                alpha=0.6, label=f"\u00b11\u03c3: \u00b1${std_d:.1f}")
axes[1].axhline(mean_d + 2 * std_d, color="red", linestyle=":", linewidth=0.7, alpha=0.5)
axes[1].axhline(mean_d - 2 * std_d, color="red", linestyle=":", linewidth=0.7,
                alpha=0.5, label=f"\u00b12\u03c3: \u00b1${2*std_d:.1f}")

# Mark >2-sigma outliers
outliers = merged[abs(merged["delta"] - mean_d) > 2 * std_d]
if not outliers.empty:
    axes[1].scatter(outliers["time"], outliers["delta"], color="red",
                    s=12, zorder=5, label=f">2\u03c3 outliers ({len(outliers)})")

axes[1].set_ylabel("Delta ($)")
axes[1].set_title("Price Delta (Binance - Chainlink)")
axes[1].legend(loc="upper left", fontsize=8)
axes[1].grid(True, alpha=0.3)

# Panel 3: Histogram + KDE with stats annotation
axes[2].hist(merged["delta"], bins=80, edgecolor="black", alpha=0.7,
             color="#009688", density=True, label="Histogram")
# KDE overlay (optional — requires scipy)
try:
    from scipy import stats as sp_stats
    kde_x = np.linspace(merged["delta"].min(), merged["delta"].max(), 300)
    kde = sp_stats.gaussian_kde(merged["delta"])
    axes[2].plot(kde_x, kde(kde_x), color="#E91E63", linewidth=1.5, label="KDE")
except ImportError:
    pass

axes[2].axvline(mean_d, color="red", linestyle="--",
                label=f"Mean: ${mean_d:.2f}")
axes[2].set_xlabel("Delta ($)")
axes[2].set_ylabel("Density")
axes[2].set_title("Distribution of Binance-Chainlink Price Delta")
axes[2].legend(loc="upper right", fontsize=8)

# Stats text box on the histogram
stats_text = (
    f"Samples: {len(merged):,}\n"
    f"Mean: ${mean_d:.2f}\n"
    f"Median: ${median_d:.2f}\n"
    f"Std: ${std_d:.2f}\n"
    f"Max: ${max_d:.2f}\n"
    f"Min: ${min_d:.2f}\n"
    f"Mean %: {merged['delta_pct'].mean():.4f}%"
)
axes[2].text(0.02, 0.95, stats_text, transform=axes[2].transAxes,
             fontsize=9, verticalalignment="top", fontfamily="monospace",
             bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

plt.tight_layout()
plt.savefig("binance_vs_chainlink.png", dpi=150)
print("Saved to binance_vs_chainlink.png")
