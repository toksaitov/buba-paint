"""
Latency Distribution: Histogram of the delay between a Binance price move
and the CLOB book adjusting. Measures the core arb window width.
"""

import sqlite3
import sys

import matplotlib.pyplot as plt
import pandas as pd

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "data/buba-paint.db"

conn = sqlite3.connect(DB_PATH)

# Get Binance ticks paired with the next CLOB tick that follows within 15 seconds.
# This measures how long it takes the CLOB to "react" after a Binance price change.
query = """
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

df = pd.read_sql_query(query, conn)
conn.close()

if df.empty:
    print("No latency data found. Run the bot longer to collect more data.")
    sys.exit(0)

print(f"Total latency samples: {len(df)}")
print(f"Mean delay: {df['delay_ms'].mean():.0f} ms")
print(f"Median delay: {df['delay_ms'].median():.0f} ms")
print(f"P95 delay: {df['delay_ms'].quantile(0.95):.0f} ms")
print(f"P99 delay: {df['delay_ms'].quantile(0.99):.0f} ms")

fig, axes = plt.subplots(1, 2, figsize=(14, 5))

# Histogram of delays
axes[0].hist(df["delay_ms"], bins=50, edgecolor="black", alpha=0.7, color="#2196F3")
axes[0].axvline(df["delay_ms"].median(), color="red", linestyle="--",
                label=f"Median: {df['delay_ms'].median():.0f} ms")
axes[0].axvline(df["delay_ms"].mean(), color="orange", linestyle="--",
                label=f"Mean: {df['delay_ms'].mean():.0f} ms")
axes[0].set_xlabel("Delay (ms)")
axes[0].set_ylabel("Frequency")
axes[0].set_title("Binance → CLOB Reaction Latency Distribution")
axes[0].legend()

# CDF
sorted_delays = df["delay_ms"].sort_values()
cdf = range(1, len(sorted_delays) + 1)
cdf_norm = [x / len(sorted_delays) for x in cdf]
axes[1].plot(sorted_delays, cdf_norm, color="#4CAF50")
axes[1].axhline(0.5, color="gray", linestyle=":", alpha=0.5)
axes[1].axhline(0.95, color="gray", linestyle=":", alpha=0.5)
axes[1].set_xlabel("Delay (ms)")
axes[1].set_ylabel("Cumulative Probability")
axes[1].set_title("Latency CDF")

plt.tight_layout()
plt.savefig("latency_distribution.png", dpi=150)
plt.show()
print("Saved to latency_distribution.png")
