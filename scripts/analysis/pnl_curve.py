"""
Simulated P&L: Per-trade bar chart (few trades) or cumulative line (many trades).

Adapts presentation based on trade count:
- < 5 trades: per-trade grouped bar chart with entry/settlement annotations
- >= 5 trades: cumulative line chart (original design)

Always includes a stats summary annotation on the chart.
"""

import sqlite3
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

if len(sys.argv) != 2:
    print("Usage: python3 scripts/analysis/pnl_curve.py <db_path>")
    sys.exit(2)

DB_PATH = sys.argv[1]

conn = sqlite3.connect(DB_PATH)

query = """
SELECT
    t.timestamp,
    t.strategy,
    t.side,
    t.entry_price,
    t.size,
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
    # Create a placeholder chart
    fig, ax = plt.subplots(figsize=(12, 6))
    ax.text(0.5, 0.5, "No trades to display\nBot has not opened/closed any positions yet",
            transform=ax.transAxes, ha="center", va="center", fontsize=16, color="gray")
    ax.set_title("Simulated P&L")
    plt.savefig("pnl_curve.png", dpi=150)
    print("Saved placeholder to pnl_curve.png")
    sys.exit(0)

df["time"] = pd.to_datetime(df["timestamp"], unit="ms")

# Stats
total_trades = len(df)
wins = len(df[df["pnl_0pct"] > 0])
win_rate = 100 * wins / total_trades if total_trades > 0 else 0

print(f"Total resolved trades: {total_trades}")
for strategy in df["strategy"].unique():
    sdf = df[df["strategy"] == strategy]
    sw = len(sdf[sdf["pnl_0pct"] > 0])
    print(f"\n{strategy}:")
    print(f"  Trades: {len(sdf)}, Wins: {sw}, Win rate: {100*sw/len(sdf):.1f}%")
    print(f"  Total P&L (0% fee): ${sdf['pnl_0pct'].sum():.2f}")
    print(f"  Total P&L (3% fee): ${sdf['pnl_3pct'].sum():.2f}")
    print(f"  Avg P&L per trade (0%): ${sdf['pnl_0pct'].mean():.2f}")

fee_cols = [("pnl_0pct", "0% fee"), ("pnl_1pct", "1% fee"),
            ("pnl_2pct", "2% fee"), ("pnl_3pct", "3% fee")]
fee_colors = ["#4CAF50", "#2196F3", "#FF9800", "#F44336"]

stats_text = (
    f"Trades: {total_trades}\n"
    f"Wins: {wins} ({win_rate:.0f}%)\n"
    f"P&L (0%): ${df['pnl_0pct'].sum():.2f}\n"
    f"P&L (1%): ${df['pnl_1pct'].sum():.2f}\n"
    f"P&L (2%): ${df['pnl_2pct'].sum():.2f}\n"
    f"P&L (3%): ${df['pnl_3pct'].sum():.2f}"
)

if total_trades < 5:
    fig, ax = plt.subplots(figsize=(14, 8))

    n = total_trades
    x = np.arange(n)
    bar_width = 0.18
    offsets = np.array([-1.5, -0.5, 0.5, 1.5]) * bar_width

    for (col, label), color, offset in zip(fee_cols, fee_colors, offsets):
        bars = ax.bar(x + offset, df[col], bar_width, label=label,
                      color=color, alpha=0.85, edgecolor="black", linewidth=0.5)

    for i, row in df.iterrows():
        won = row["pnl_0pct"] > 0
        result = "WIN" if won else "LOSS"
        label_text = (
            f"{row['strategy']}\n"
            f"{row['side']} @ {row['entry_price']:.3f}\n"
            f"settle: {row['settlement_price']:.0f}\n"
            f"{result}"
        )
        y_pos = max(row["pnl_0pct"], row["pnl_3pct"])
        y_pos = y_pos + abs(y_pos) * 0.15 + 1
        ax.text(i, y_pos, label_text, ha="center", va="bottom",
                fontsize=8, fontfamily="monospace",
                color="green" if won else "red", fontweight="bold")

    ax.axhline(0, color="black", linestyle="-", linewidth=0.8)
    ax.set_xticks(x)
    ax.set_xticklabels([f"Trade {i+1}\n{t.strftime('%H:%M')}"
                        for i, t in enumerate(df["time"])], fontsize=9)
    ax.set_ylabel("P&L ($)")
    ax.set_title(f"Per-Trade P&L ({total_trades} trades)")
    ax.legend(loc="upper left")
    ax.grid(True, alpha=0.3, axis="y")

    ax.text(0.98, 0.95, stats_text, transform=ax.transAxes,
            fontsize=9, verticalalignment="top", horizontalalignment="right",
            fontfamily="monospace",
            bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

else:
    fig, axes = plt.subplots(2, 1, figsize=(14, 10))

    for (col, label), color in zip(fee_cols, fee_colors):
        cumulative = df[col].cumsum()
        axes[0].plot(df["time"], cumulative, label=label, color=color, linewidth=1.5)

    axes[0].axhline(0, color="black", linestyle="-", linewidth=0.5)
    axes[0].set_ylabel("Cumulative P&L ($)")
    axes[0].set_title("Combined Cumulative P&L -- All Strategies")
    axes[0].legend()
    axes[0].grid(True, alpha=0.3)

    axes[0].text(0.02, 0.95, stats_text, transform=axes[0].transAxes,
                 fontsize=9, verticalalignment="top", fontfamily="monospace",
                 bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.9))

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
print("Saved to pnl_curve.png")
