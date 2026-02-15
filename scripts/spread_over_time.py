"""
Spread Over Time: Time series of (UP ask + DOWN ask) across all 5-minute windows.
Shows how often the spread dips below $1.00 and by how much.
"""

import sqlite3
import sys
from datetime import datetime

import matplotlib.pyplot as plt
import pandas as pd

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "data/buba-paint.db"

conn = sqlite3.connect(DB_PATH)

query = """
SELECT
    u.timestamp,
    u.ask AS up_ask,
    d.ask AS down_ask,
    u.ask + d.ask AS total_ask
FROM tick_data u
JOIN tick_data d
    ON d.source = 'clob_down'
    AND ABS(d.timestamp - u.timestamp) < 1500
WHERE u.source = 'clob_up'
    AND u.ask IS NOT NULL AND u.ask > 0
    AND d.ask IS NOT NULL AND d.ask > 0
ORDER BY u.timestamp
"""

df = pd.read_sql_query(query, conn)
conn.close()

if df.empty:
    print("No CLOB spread data found. Run the bot longer to collect data.")
    sys.exit(0)

df["time"] = pd.to_datetime(df["timestamp"], unit="ms")

below_1 = df[df["total_ask"] < 1.0]
below_098 = df[df["total_ask"] < 0.98]

print(f"Total spread samples: {len(df)}")
print(f"Mean total ask: {df['total_ask'].mean():.4f}")
print(f"Min total ask: {df['total_ask'].min():.4f}")
print(f"Samples below $1.00: {len(below_1)} ({100*len(below_1)/len(df):.1f}%)")
print(f"Samples below $0.98: {len(below_098)} ({100*len(below_098)/len(df):.1f}%)")

fig, axes = plt.subplots(2, 1, figsize=(14, 8), sharex=True)

# Time series of total ask
axes[0].plot(df["time"], df["total_ask"], linewidth=0.5, alpha=0.7, color="#2196F3")
axes[0].axhline(1.0, color="red", linestyle="--", linewidth=1, label="$1.00 (fair value)")
axes[0].axhline(0.98, color="orange", linestyle="--", linewidth=1, label="$0.98 (threshold)")
axes[0].set_ylabel("UP Ask + DOWN Ask ($)")
axes[0].set_title("Combined Ask Price Over Time (UP + DOWN)")
axes[0].legend()
axes[0].grid(True, alpha=0.3)

# Individual UP and DOWN asks
axes[1].plot(df["time"], df["up_ask"], linewidth=0.5, alpha=0.7, color="#4CAF50", label="UP ask")
axes[1].plot(df["time"], df["down_ask"], linewidth=0.5, alpha=0.7, color="#F44336", label="DOWN ask")
axes[1].axhline(0.5, color="gray", linestyle=":", alpha=0.5, label="50/50")
axes[1].set_xlabel("Time")
axes[1].set_ylabel("Ask Price ($)")
axes[1].set_title("Individual Token Ask Prices")
axes[1].legend()
axes[1].grid(True, alpha=0.3)

plt.tight_layout()
plt.savefig("spread_over_time.png", dpi=150)
plt.show()
print("Saved to spread_over_time.png")
