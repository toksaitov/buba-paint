"""
Simulated P&L Curve: Cumulative P&L over time for each strategy,
with separate lines for each fee assumption (0%, 1%, 2%, 3%).
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
    t.timestamp,
    t.strategy,
    t.side,
    t.entry_price,
    r.settlement_price,
    r.pnl_0pct,
    r.pnl_1pct,
    r.pnl_2pct,
    r.pnl_3pct
FROM trade_results r
JOIN simulated_trades t ON r.trade_id = t.id
ORDER BY t.timestamp
"""

df = pd.read_sql_query(query, conn)
conn.close()

if df.empty:
    print("No trade results found. Run the bot longer to collect data.")
    sys.exit(0)

df["time"] = pd.to_datetime(df["timestamp"], unit="ms")

print(f"Total resolved trades: {len(df)}")
for strategy in df["strategy"].unique():
    sdf = df[df["strategy"] == strategy]
    wins = len(sdf[sdf["pnl_0pct"] > 0])
    print(f"\n{strategy}:")
    print(f"  Trades: {len(sdf)}, Wins: {wins}, Win rate: {100*wins/len(sdf):.1f}%")
    print(f"  Total P&L (0% fee): ${sdf['pnl_0pct'].sum():.2f}")
    print(f"  Total P&L (3% fee): ${sdf['pnl_3pct'].sum():.2f}")
    print(f"  Avg P&L per trade (0%): ${sdf['pnl_0pct'].mean():.2f}")

fig, axes = plt.subplots(2, 1, figsize=(14, 10))

fee_cols = [("pnl_0pct", "0% fee"), ("pnl_1pct", "1% fee"),
            ("pnl_2pct", "2% fee"), ("pnl_3pct", "3% fee")]
colors = ["#4CAF50", "#2196F3", "#FF9800", "#F44336"]

# Combined P&L
for (col, label), color in zip(fee_cols, colors):
    cumulative = df[col].cumsum()
    axes[0].plot(df["time"], cumulative, label=label, color=color, linewidth=1.5)

axes[0].axhline(0, color="black", linestyle="-", linewidth=0.5)
axes[0].set_ylabel("Cumulative P&L ($)")
axes[0].set_title("Combined Cumulative P&L — All Strategies")
axes[0].legend()
axes[0].grid(True, alpha=0.3)

# Per-strategy P&L (0% fee)
for strategy in df["strategy"].unique():
    sdf = df[df["strategy"] == strategy].copy()
    sdf["cum_pnl"] = sdf["pnl_0pct"].cumsum()
    axes[1].plot(sdf["time"], sdf["cum_pnl"], label=strategy, linewidth=1.5)

axes[1].axhline(0, color="black", linestyle="-", linewidth=0.5)
axes[1].set_xlabel("Time")
axes[1].set_ylabel("Cumulative P&L ($)")
axes[1].set_title("Per-Strategy Cumulative P&L (0% Fee)")
axes[1].legend()
axes[1].grid(True, alpha=0.3)

plt.tight_layout()
plt.savefig("pnl_curve.png", dpi=150)
plt.show()
print("Saved to pnl_curve.png")
