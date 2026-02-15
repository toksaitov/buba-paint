"""
Binance vs Chainlink Delta: Time series of the price difference between
the two feeds, to understand oracle lag near window boundaries.
"""

import sqlite3
import sys

import matplotlib.pyplot as plt
import pandas as pd

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "data/buba-paint.db"

conn = sqlite3.connect(DB_PATH)

# Get both feeds and align by timestamp (nearest-second buckets)
binance_query = """
SELECT timestamp, price AS binance_price
FROM tick_data
WHERE source = 'binance' AND price IS NOT NULL
ORDER BY timestamp
"""

chainlink_query = """
SELECT timestamp, price AS chainlink_price
FROM tick_data
WHERE source = 'chainlink' AND price IS NOT NULL
ORDER BY timestamp
"""

markets_query = """
SELECT start_time, end_time, question FROM markets ORDER BY start_time
"""

bf = pd.read_sql_query(binance_query, conn)
cf = pd.read_sql_query(chainlink_query, conn)
mf = pd.read_sql_query(markets_query, conn)
conn.close()

if bf.empty or cf.empty:
    print("Insufficient data. Need both Binance and Chainlink ticks.")
    sys.exit(0)

# Bucket to 1-second intervals and merge
bf["second"] = (bf["timestamp"] // 1000) * 1000
cf["second"] = (cf["timestamp"] // 1000) * 1000

# Take last value per second bucket
bf_agg = bf.groupby("second").last().reset_index()[["second", "binance_price"]]
cf_agg = cf.groupby("second").last().reset_index()[["second", "chainlink_price"]]

merged = pd.merge(bf_agg, cf_agg, on="second", how="inner")
merged["time"] = pd.to_datetime(merged["second"], unit="ms")
merged["delta"] = merged["binance_price"] - merged["chainlink_price"]
merged["delta_pct"] = (merged["delta"] / merged["chainlink_price"]) * 100

print(f"Merged samples: {len(merged)}")
print(f"Mean delta: ${merged['delta'].mean():.2f} ({merged['delta_pct'].mean():.4f}%)")
print(f"Std delta: ${merged['delta'].std():.2f}")
print(f"Max delta: ${merged['delta'].max():.2f}")
print(f"Min delta: ${merged['delta'].min():.2f}")

fig, axes = plt.subplots(3, 1, figsize=(14, 12))

# Price comparison
axes[0].plot(merged["time"], merged["binance_price"], linewidth=0.5,
             alpha=0.8, color="#2196F3", label="Binance")
axes[0].plot(merged["time"], merged["chainlink_price"], linewidth=0.5,
             alpha=0.8, color="#FF9800", label="Chainlink")

# Shade market windows
for _, row in mf.iterrows():
    start = pd.to_datetime(row["start_time"], unit="ms")
    end = pd.to_datetime(row["end_time"], unit="ms")
    axes[0].axvspan(start, end, alpha=0.05, color="green")

axes[0].set_ylabel("BTC Price ($)")
axes[0].set_title("Binance vs Chainlink BTC Price")
axes[0].legend()
axes[0].grid(True, alpha=0.3)

# Absolute delta
axes[1].plot(merged["time"], merged["delta"], linewidth=0.5, alpha=0.7, color="#9C27B0")
axes[1].axhline(0, color="black", linestyle="-", linewidth=0.5)
axes[1].fill_between(merged["time"], merged["delta"], 0, alpha=0.2, color="#9C27B0")
axes[1].set_ylabel("Delta ($)")
axes[1].set_title("Price Delta (Binance - Chainlink)")
axes[1].grid(True, alpha=0.3)

# Histogram of deltas
axes[2].hist(merged["delta"], bins=80, edgecolor="black", alpha=0.7, color="#009688")
axes[2].axvline(merged["delta"].mean(), color="red", linestyle="--",
                label=f"Mean: ${merged['delta'].mean():.2f}")
axes[2].set_xlabel("Delta ($)")
axes[2].set_ylabel("Frequency")
axes[2].set_title("Distribution of Binance-Chainlink Price Delta")
axes[2].legend()

plt.tight_layout()
plt.savefig("binance_vs_chainlink.png", dpi=150)
plt.show()
print("Saved to binance_vs_chainlink.png")
