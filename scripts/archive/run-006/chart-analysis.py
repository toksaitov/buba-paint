#!/usr/bin/env python3
"""Custom deep-analysis charts for run 006."""

import sqlite3
from datetime import datetime, timezone
from pathlib import Path
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import matplotlib.ticker as mticker
import numpy as np

DB = "runs/006/buba-paint.db"
OUT = Path("runs/006")
conn = sqlite3.connect(DB)
conn.row_factory = sqlite3.Row

def ts(ms):
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc)

# ── Load data ────────────────────────────────────────────────────────────
trades = conn.execute("""
    SELECT st.*, tr.pnl_2pct, tr.resolved_at, tr.settlement_price
    FROM simulated_trades st
    JOIN trade_results tr ON tr.trade_id = st.id
    ORDER BY tr.resolved_at
""").fetchall()

bal_rows = conn.execute(
    "SELECT timestamp, balance FROM balance_log ORDER BY timestamp"
).fetchall()

signals = conn.execute(
    "SELECT timestamp, strategy, direction FROM signals ORDER BY timestamp"
).fetchall()

conn.close()

bal_times = [ts(r["timestamp"]) for r in bal_rows]
bal_vals = [r["balance"] for r in bal_rows]

# ── FIGURE 1: 6-panel overview ──────────────────────────────────────────
fig, axes = plt.subplots(3, 2, figsize=(20, 18))
fig.suptitle("Run 006 (v0.4) Deep Analysis - $200 → $4,765 (+2,283%)",
             fontsize=16, fontweight="bold", y=0.98)

# ── Panel 1: Equity curve (clean) ───────────────────────────────────────
ax = axes[0, 0]
ax.fill_between(bal_times, 200, bal_vals,
                where=[b >= 200 for b in bal_vals], alpha=0.15, color="green", step="post")
ax.fill_between(bal_times, 200, bal_vals,
                where=[b < 200 for b in bal_vals], alpha=0.15, color="red", step="post")
ax.step(bal_times, bal_vals, where="post", color="#2196F3", linewidth=1.5)
ax.axhline(200, color="gray", linewidth=0.8, linestyle="--", alpha=0.5)

# Mark the peak
peak_idx = bal_vals.index(max(bal_vals))
ax.annotate(f"Peak ${bal_vals[peak_idx]:,.0f}", (bal_times[peak_idx], bal_vals[peak_idx]),
            textcoords="offset points", xytext=(-60, 10), fontsize=10, fontweight="bold",
            color="green", arrowprops=dict(arrowstyle="->", color="green"))
# Mark the end
ax.annotate(f"End ${bal_vals[-1]:,.0f}", (bal_times[-1], bal_vals[-1]),
            textcoords="offset points", xytext=(-80, -25), fontsize=10, fontweight="bold",
            color="#2196F3", arrowprops=dict(arrowstyle="->", color="#2196F3"))

ax.set_title("Equity Curve", fontweight="bold", fontsize=12)
ax.set_ylabel("Balance (USD)")
ax.yaxis.set_major_formatter(mticker.FuncFormatter(lambda x, p: f"${x:,.0f}"))
ax.grid(True, alpha=0.3)
ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d", tz=timezone.utc))

# ── Panel 2: Daily PnL bars ─────────────────────────────────────────────
ax = axes[0, 1]
daily = {}
for t in trades:
    day = ts(t["resolved_at"]).strftime("%b %d")
    daily[day] = daily.get(day, 0) + t["pnl_2pct"]

days = list(daily.keys())
pnls = list(daily.values())
colors = ["#4CAF50" if p > 0 else "#F44336" for p in pnls]
bars = ax.bar(days, pnls, color=colors, edgecolor="black", linewidth=0.5)
for bar, val in zip(bars, pnls):
    ax.text(bar.get_x() + bar.get_width()/2,
            bar.get_height() + (50 if val > 0 else -50),
            f"${val:+,.0f}", ha="center", va="bottom" if val > 0 else "top",
            fontsize=9, fontweight="bold")
ax.set_title("Daily P&L (2% fee tier)", fontweight="bold", fontsize=12)
ax.set_ylabel("P&L (USD)")
ax.axhline(0, color="black", linewidth=0.5)
ax.grid(True, alpha=0.3, axis="y")
ax.tick_params(axis="x", rotation=45)

# ── Panel 3: Hourly win rate ────────────────────────────────────────────
ax = axes[1, 0]
hourly_w = [0]*24
hourly_total = [0]*24
hourly_pnl = [0]*24
for t in trades:
    h = ts(t["resolved_at"]).hour
    hourly_total[h] += 1
    if t["pnl_2pct"] > 0:
        hourly_w[h] += 1
    hourly_pnl[h] += t["pnl_2pct"]

hourly_wr = [100*hourly_w[h]/hourly_total[h] if hourly_total[h] > 0 else 0 for h in range(24)]
bar_colors = ["#4CAF50" if wr >= 55 else "#FFC107" if wr >= 50 else "#F44336" for wr in hourly_wr]
bars = ax.bar(range(24), hourly_wr, color=bar_colors, edgecolor="black", linewidth=0.5, alpha=0.8)
ax.axhline(50, color="gray", linewidth=1, linestyle="--", alpha=0.5)
ax.axhline(56.5, color="blue", linewidth=1, linestyle=":", alpha=0.5, label="Avg WR 56.5%")
for h in range(24):
    if hourly_total[h] > 0:
        ax.text(h, hourly_wr[h] + 1, f"{hourly_total[h]}", ha="center", fontsize=7, color="gray")

# Shade dangerous hours
ax.axvspan(4.5, 9.5, alpha=0.08, color="red", label="Danger zone 05-09 UTC")
ax.axvspan(12.5, 17.5, alpha=0.08, color="green", label="Hot zone 13-17 UTC")

ax.set_title("Win Rate by Hour (UTC) - number = trade count", fontweight="bold", fontsize=12)
ax.set_ylabel("Win Rate %")
ax.set_xlabel("Hour (UTC)")
ax.set_xticks(range(24))
ax.set_ylim(0, 105)
ax.legend(fontsize=8, loc="lower right")
ax.grid(True, alpha=0.3, axis="y")

# ── Panel 4: Hourly PnL ────────────────────────────────────────────────
ax = axes[1, 1]
pnl_colors = ["#4CAF50" if p > 0 else "#F44336" for p in hourly_pnl]
bars = ax.bar(range(24), hourly_pnl, color=pnl_colors, edgecolor="black", linewidth=0.5, alpha=0.8)
ax.axvspan(4.5, 9.5, alpha=0.08, color="red")
ax.axvspan(12.5, 17.5, alpha=0.08, color="green")
for h in range(24):
    if abs(hourly_pnl[h]) > 500:
        ax.text(h, hourly_pnl[h] + (100 if hourly_pnl[h] > 0 else -100),
                f"${hourly_pnl[h]:+,.0f}", ha="center", fontsize=7, fontweight="bold")
ax.set_title("P&L by Hour (USD)", fontweight="bold", fontsize=12)
ax.set_ylabel("P&L (USD)")
ax.set_xlabel("Hour (UTC)")
ax.set_xticks(range(24))
ax.axhline(0, color="black", linewidth=0.5)
ax.grid(True, alpha=0.3, axis="y")

# ── Panel 5: Entry price vs WR (latency-arb) ───────────────────────────
ax = axes[2, 0]
buckets = {
    "0.30-0.35": (0.30, 0.35), "0.35-0.40": (0.35, 0.40),
    "0.40-0.45": (0.40, 0.45), "0.45-0.50": (0.45, 0.50),
    "0.50-0.55": (0.50, 0.55), "0.55-0.60": (0.55, 0.60),
}
bucket_w = {b: 0 for b in buckets}
bucket_total = {b: 0 for b in buckets}
bucket_pnl = {b: 0.0 for b in buckets}
for t in trades:
    if t["strategy"] != "latency-arb":
        continue
    ep = t["entry_price"]
    for bname, (lo, hi) in buckets.items():
        if lo <= ep < hi:
            bucket_total[bname] += 1
            if t["pnl_2pct"] > 0:
                bucket_w[bname] += 1
            bucket_pnl[bname] += t["pnl_2pct"]
            break

bnames = list(buckets.keys())
bwr = [100*bucket_w[b]/bucket_total[b] if bucket_total[b] > 0 else 0 for b in bnames]
bpnl = [bucket_pnl[b] for b in bnames]
btot = [bucket_total[b] for b in bnames]

x = np.arange(len(bnames))
width = 0.4
bars1 = ax.bar(x - width/2, bwr, width, color="#2196F3", alpha=0.7, label="Win Rate %")
ax.set_ylabel("Win Rate %", color="#2196F3")
ax.set_ylim(0, 80)
ax.axhline(50, color="gray", linewidth=0.8, linestyle="--", alpha=0.5)

ax2 = ax.twinx()
bars2 = ax2.bar(x + width/2, bpnl, width,
                color=["#4CAF50" if p > 0 else "#F44336" for p in bpnl], alpha=0.7, label="P&L $")
ax2.set_ylabel("P&L (USD)")

for i, (wr, tot) in enumerate(zip(bwr, btot)):
    ax.text(i - width/2, wr + 1, f"{wr:.0f}%\n({tot})", ha="center", fontsize=8, fontweight="bold")
for i, p in enumerate(bpnl):
    ax2.text(i + width/2, p + (30 if p > 0 else -30), f"${p:+,.0f}",
             ha="center", fontsize=8, fontweight="bold", va="bottom" if p > 0 else "top")

ax.set_title("Latency-Arb: Win Rate & P&L by Entry Price", fontweight="bold", fontsize=12)
ax.set_xticks(x)
ax.set_xticklabels(bnames)
ax.grid(True, alpha=0.3, axis="y")

# ── Panel 6: UP vs DOWN side (latency-arb) ─────────────────────────────
ax = axes[2, 1]
side_data = {"UP": {"W": 0, "L": 0, "pnl": 0}, "DOWN": {"W": 0, "L": 0, "pnl": 0}}
for t in trades:
    if t["strategy"] != "latency-arb":
        continue
    s = t["side"]
    side_data[s]["pnl"] += t["pnl_2pct"]
    if t["pnl_2pct"] > 0:
        side_data[s]["W"] += 1
    else:
        side_data[s]["L"] += 1

sides = ["UP", "DOWN"]
wr = [100*side_data[s]["W"]/(side_data[s]["W"]+side_data[s]["L"]) for s in sides]
pnl = [side_data[s]["pnl"] for s in sides]
total = [side_data[s]["W"]+side_data[s]["L"] for s in sides]

x = np.arange(2)
width = 0.35
bars1 = ax.bar(x - width/2, wr, width, color=["#4CAF50", "#F44336"], alpha=0.7, label="Win Rate %")
ax.set_ylabel("Win Rate %")
ax.set_ylim(0, 75)
ax.axhline(50, color="gray", linewidth=0.8, linestyle="--", alpha=0.5)

ax2 = ax.twinx()
bars2 = ax2.bar(x + width/2, pnl, width,
                color=["#FFB74D" if p > 0 else "#EF9A9A" for p in pnl], alpha=0.8, label="P&L $")
ax2.set_ylabel("P&L (USD)")

for i, (w, t) in enumerate(zip(wr, total)):
    ax.text(i - width/2, w + 1, f"{w:.1f}%\n({t} trades)", ha="center", fontsize=10, fontweight="bold")
for i, p in enumerate(pnl):
    ax2.text(i + width/2, p + (100 if p > 0 else -100), f"${p:+,.0f}",
             ha="center", fontsize=10, fontweight="bold", va="bottom" if p > 0 else "top")

ax.set_title("Latency-Arb: UP vs DOWN Side", fontweight="bold", fontsize=12)
ax.set_xticks(x)
ax.set_xticklabels(["Bet UP (BTC rises)", "Bet DOWN (BTC falls)"])
ax.grid(True, alpha=0.3, axis="y")

plt.tight_layout(rect=[0, 0, 1, 0.96])
out = OUT / "run-006-analysis.png"
plt.savefig(out, dpi=150, bbox_inches="tight", facecolor="white")
print(f"Saved: {out}")
plt.close()

# ── FIGURE 2: Drawdown + volatility ─────────────────────────────────────
fig, axes = plt.subplots(2, 2, figsize=(20, 12))
fig.suptitle("Run 006 - Risk Analysis", fontsize=16, fontweight="bold", y=0.98)

# Panel 1: Drawdown over time
ax = axes[0, 0]
peak = bal_vals[0]
dd = []
dd_times = []
for i, (t, b) in enumerate(zip(bal_times, bal_vals)):
    if b > peak:
        peak = b
    dd.append(-(peak - b) / peak * 100 if peak > 0 else 0)
    dd_times.append(t)
ax.fill_between(dd_times, dd, 0, alpha=0.3, color="red", step="post")
ax.step(dd_times, dd, where="post", color="red", linewidth=1)
ax.axhline(-50, color="black", linewidth=1, linestyle="--", alpha=0.3, label="-50% DD")
ax.set_title("Drawdown from Peak", fontweight="bold", fontsize=12)
ax.set_ylabel("Drawdown %")
ax.legend(fontsize=9)
ax.grid(True, alpha=0.3)
ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d", tz=timezone.utc))

# Panel 2: Bet sizes over time
ax = axes[0, 1]
trade_times = [ts(t["resolved_at"]) for t in trades]
trade_costs = [t["size"] * t["entry_price"] for t in trades]
trade_pnls = [t["pnl_2pct"] for t in trades]
colors = ["green" if p > 0 else "red" for p in trade_pnls]
ax.scatter(trade_times, trade_costs, c=colors, s=30, alpha=0.6, edgecolors="black", linewidth=0.3)
ax.set_title("Bet Size Over Time (green=win, red=loss)", fontweight="bold", fontsize=12)
ax.set_ylabel("Bet Size (USD)")
ax.yaxis.set_major_formatter(mticker.FuncFormatter(lambda x, p: f"${x:,.0f}"))
ax.grid(True, alpha=0.3)
ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d", tz=timezone.utc))

# Panel 3: Rolling 20-trade win rate
ax = axes[1, 0]
window = 20
rolling_wr = []
rolling_times = []
for i in range(window, len(trades)):
    chunk = trades[i-window:i]
    wr = sum(1 for t in chunk if t["pnl_2pct"] > 0) / window * 100
    rolling_wr.append(wr)
    rolling_times.append(ts(trades[i]["resolved_at"]))
ax.plot(rolling_times, rolling_wr, color="#2196F3", linewidth=1.5)
ax.axhline(56.5, color="green", linewidth=1, linestyle="--", alpha=0.5, label="Overall 56.5%")
ax.axhline(50, color="gray", linewidth=1, linestyle="--", alpha=0.3)
ax.fill_between(rolling_times, 50, rolling_wr,
                where=[wr >= 50 for wr in rolling_wr], alpha=0.1, color="green")
ax.fill_between(rolling_times, 50, rolling_wr,
                where=[wr < 50 for wr in rolling_wr], alpha=0.1, color="red")
ax.set_title(f"Rolling {window}-Trade Win Rate", fontweight="bold", fontsize=12)
ax.set_ylabel("Win Rate %")
ax.set_ylim(20, 90)
ax.legend(fontsize=9)
ax.grid(True, alpha=0.3)
ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d", tz=timezone.utc))

# Panel 4: Cumulative PnL by strategy
ax = axes[1, 1]
lat_cum = []
spr_cum = []
lat_times = []
spr_times = []
lat_run = 0
spr_run = 0
for t in trades:
    if t["strategy"] == "latency-arb":
        lat_run += t["pnl_2pct"]
        lat_cum.append(lat_run)
        lat_times.append(ts(t["resolved_at"]))
    else:
        spr_run += t["pnl_2pct"]
        spr_cum.append(spr_run)
        spr_times.append(ts(t["resolved_at"]))

ax.plot(lat_times, lat_cum, color="#2196F3", linewidth=2, label=f"Latency-Arb ${lat_run:+,.0f}")
ax.plot(spr_times, spr_cum, color="#FF9800", linewidth=2, label=f"Spread-Cap ${spr_run:+,.0f}")
ax.axhline(0, color="black", linewidth=0.5)
ax.set_title("Cumulative P&L by Strategy", fontweight="bold", fontsize=12)
ax.set_ylabel("Cumulative P&L (USD)")
ax.yaxis.set_major_formatter(mticker.FuncFormatter(lambda x, p: f"${x:+,.0f}"))
ax.legend(fontsize=10, loc="upper left")
ax.grid(True, alpha=0.3)
ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d", tz=timezone.utc))

plt.tight_layout(rect=[0, 0, 1, 0.96])
out2 = OUT / "run-006-risk.png"
plt.savefig(out2, dpi=150, bbox_inches="tight", facecolor="white")
print(f"Saved: {out2}")
plt.close()

print("\nDone!")
