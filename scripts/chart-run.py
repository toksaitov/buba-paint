#!/usr/bin/env python3
"""Generate performance charts for a buba-paint run.

Usage: python3 chart-run.py <db_path> [starting_balance]
  - starting_balance defaults to 200 if balance_log table exists, else 1000
"""

import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import matplotlib.ticker as mticker

DB_PATH = sys.argv[1] if len(sys.argv) > 1 else "runs/004/buba-paint.db"
STARTING_BAL_ARG = float(sys.argv[2]) if len(sys.argv) > 2 else None
OUT_DIR = Path(DB_PATH).parent
RUN_NAME = OUT_DIR.name  # e.g. "004"

conn = sqlite3.connect(DB_PATH)
conn.row_factory = sqlite3.Row

# ── helpers ──────────────────────────────────────────────────────────────
def ts_to_dt(ms):
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc)

def fmt_ax(ax, title, ylabel):
    ax.set_title(title, fontsize=13, fontweight="bold", pad=10)
    ax.set_ylabel(ylabel, fontsize=10)
    ax.grid(True, alpha=0.3, linewidth=0.5)
    ax.xaxis.set_major_formatter(mdates.DateFormatter("%b %d\n%H:%M", tz=timezone.utc))
    ax.tick_params(labelsize=9)

def table_exists(name):
    r = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)
    ).fetchone()
    return r is not None

# ── 1. load data ─────────────────────────────────────────────────────────
# Trades (always present)
trades = conn.execute("""
    SELECT st.id, st.timestamp, st.strategy, st.side, st.entry_price, st.size, st.status,
           tr.settlement_price, tr.pnl_2pct, tr.pnl_0pct, tr.pnl_1pct, tr.pnl_3pct, tr.resolved_at
    FROM simulated_trades st
    LEFT JOIN trade_results tr ON tr.trade_id = st.id
    ORDER BY st.id
""").fetchall()

# Balance log — use if available, otherwise synthesize from trades
has_balance_log = table_exists("balance_log")

if has_balance_log:
    bal_rows = conn.execute(
        "SELECT timestamp, balance FROM balance_log ORDER BY timestamp"
    ).fetchall()
    bal_times = [ts_to_dt(r["timestamp"]) for r in bal_rows]
    bal_vals  = [r["balance"] for r in bal_rows]
    starting_bal = bal_vals[0]
else:
    # Synthesize: assume starting balance, apply P&L from resolved trades
    starting_bal = STARTING_BAL_ARG if STARTING_BAL_ARG is not None else 1000.0
    bal_times = []
    bal_vals  = []
    # Get first tick timestamp for the init point
    first_tick = conn.execute(
        "SELECT MIN(timestamp) as ts FROM tick_data"
    ).fetchone()
    if first_tick and first_tick["ts"]:
        bal_times.append(ts_to_dt(first_tick["ts"]))
        bal_vals.append(starting_bal)
    running = starting_bal
    for t in trades:
        if t["resolved_at"] is not None and t["pnl_2pct"] is not None:
            # approximate the actual balance change: pnl_2pct is the net P&L
            # but for pre-v0.2 runs, size was always 100 and there's no Kelly
            # so the balance change ~ pnl_2pct
            running += t["pnl_2pct"]
            bal_times.append(ts_to_dt(t["resolved_at"]))
            bal_vals.append(running)

# Sampled BTC price (every ~2 min)
btc_rows = conn.execute("""
    SELECT timestamp, price FROM tick_data
    WHERE source='binance' AND price IS NOT NULL
    AND (timestamp / 1000) % 120 < 2
    ORDER BY timestamp
""").fetchall()
btc_times  = [ts_to_dt(r["timestamp"]) for r in btc_rows]
btc_prices = [r["price"] for r in btc_rows]

# CLOB spreads (sampled)
clob_up = conn.execute("""
    SELECT timestamp, bid, ask FROM tick_data
    WHERE source='clob_up' AND bid IS NOT NULL
    AND (timestamp / 1000) % 120 < 2
    ORDER BY timestamp
""").fetchall()
clob_down = conn.execute("""
    SELECT timestamp, bid, ask FROM tick_data
    WHERE source='clob_down' AND bid IS NOT NULL
    AND (timestamp / 1000) % 120 < 2
    ORDER BY timestamp
""").fetchall()

# Signals (all)
signals = conn.execute(
    "SELECT timestamp, strategy, direction FROM signals ORDER BY timestamp"
).fetchall()

conn.close()

# ── summary stats ────────────────────────────────────────────────────────
current_bal  = bal_vals[-1] if bal_vals else starting_bal
total_pnl    = current_bal - starting_bal
pnl_pct      = (total_pnl / starting_bal) * 100 if starting_bal > 0 else 0
n_trades     = len(trades)
wins         = sum(1 for t in trades if t["pnl_2pct"] is not None and t["pnl_2pct"] > 0)
losses       = sum(1 for t in trades if t["pnl_2pct"] is not None and t["pnl_2pct"] <= 0)
win_rate     = (wins / n_trades * 100) if n_trades > 0 else 0

# Max drawdown
peak = starting_bal
max_dd = 0
for b in bal_vals:
    if b > peak:
        peak = b
    dd = (peak - b) / peak if peak > 0 else 0
    if dd > max_dd:
        max_dd = dd

# Duration
run_start = bal_times[0] if bal_times else (ts_to_dt(btc_rows[0]["timestamp"]) if btc_rows else datetime.now(timezone.utc))
run_end   = ts_to_dt(btc_rows[-1]["timestamp"]) if btc_rows else (bal_times[-1] if bal_times else run_start)
duration_h = (run_end - run_start).total_seconds() / 3600

n_signals = len(signals)

print(f"Run {RUN_NAME} Summary")
print(f"  Duration:     {duration_h:.1f} hours")
print(f"  Balance:      ${starting_bal:.2f} → ${current_bal:.2f}")
print(f"  P&L:          ${total_pnl:+.2f} ({pnl_pct:+.1f}%)")
print(f"  Trades:       {n_trades} ({wins}W / {losses}L) — {win_rate:.0f}% win rate")
print(f"  Max Drawdown: {max_dd*100:.1f}%")
print(f"  Signals:      {n_signals}")
if btc_prices:
    print(f"  BTC Range:    ${min(btc_prices):,.2f} – ${max(btc_prices):,.2f}")
print()

# ── 2. CHART: 4-panel dashboard ─────────────────────────────────────────
fig, axes = plt.subplots(4, 1, figsize=(16, 20), gridspec_kw={"height_ratios": [3, 3, 2, 2]})
fig.suptitle(
    f"Run {RUN_NAME}  —  {run_start.strftime('%b %d %H:%M')} → {run_end.strftime('%b %d %H:%M UTC')}  "
    f"({duration_h:.1f}h)    P&L: ${total_pnl:+.2f} ({pnl_pct:+.1f}%)",
    fontsize=15, fontweight="bold", y=0.98
)

# ── Panel 1: Equity Curve ────────────────────────────────────────────────
ax1 = axes[0]
fmt_ax(ax1, "Equity Curve", "Balance (USD)")

if bal_vals:
    ax1.axhline(starting_bal, color="gray", linewidth=0.8, linestyle="--", alpha=0.5, label=f"Start ${starting_bal:.0f}")
    ax1.fill_between(bal_times, starting_bal, bal_vals,
                     where=[b >= starting_bal for b in bal_vals],
                     alpha=0.15, color="green", step="post")
    ax1.fill_between(bal_times, starting_bal, bal_vals,
                     where=[b < starting_bal for b in bal_vals],
                     alpha=0.15, color="red", step="post")
    ax1.step(bal_times, bal_vals, where="post", color="#2196F3", linewidth=2.5, label=f"Balance ${current_bal:.2f}")

    # Mark each trade result
    for t in trades:
        if t["resolved_at"] is None:
            continue
        dt = ts_to_dt(t["resolved_at"])
        pnl = t["pnl_2pct"]
        if pnl is None:
            continue
        color = "green" if pnl > 0 else "red"
        marker = "^" if pnl > 0 else "v"
        bal_at = None
        for i, bt in enumerate(bal_times):
            if bt >= dt:
                bal_at = bal_vals[i]
                break
        if bal_at is None:
            bal_at = bal_vals[-1]
        ax1.scatter(dt, bal_at, color=color, marker=marker, s=120, zorder=5,
                    edgecolors="black", linewidth=0.5)
        label = f"{'W' if pnl > 0 else 'L'} ${pnl:+.1f}"
        ax1.annotate(label, (dt, bal_at), textcoords="offset points",
                     xytext=(8, 12 if pnl > 0 else -18), fontsize=8, fontweight="bold", color=color)

    ax1.legend(loc="upper left", fontsize=9)
ax1.yaxis.set_major_formatter(mticker.FormatStrFormatter("$%.0f"))

# ── Panel 2: BTC Price + Trade Markers ───────────────────────────────────
ax2 = axes[1]
fmt_ax(ax2, "BTC/USD Price + Trade Entries", "Price (USD)")
if btc_prices:
    ax2.plot(btc_times, btc_prices, color="#FF9800", linewidth=1, alpha=0.8, label="BTC (Binance)")

# Mark trade entries
for t in trades:
    dt = ts_to_dt(t["timestamp"])
    closest_price = None
    min_diff = float("inf")
    for i, bt in enumerate(btc_times):
        diff = abs((bt - dt).total_seconds())
        if diff < min_diff:
            min_diff = diff
            closest_price = btc_prices[i]
    if closest_price is None:
        continue

    is_win = t["pnl_2pct"] is not None and t["pnl_2pct"] > 0
    color = "green" if is_win else "red"
    marker = "^" if t["side"] == "UP" else "v"
    ax2.scatter(dt, closest_price, color=color, marker=marker, s=150, zorder=5,
                edgecolors="black", linewidth=0.8)
    ax2.annotate(
        f"{t['strategy'][:3].upper()}\n{t['side']}",
        (dt, closest_price),
        textcoords="offset points",
        xytext=(10, 10 if t["side"] == "UP" else -18),
        fontsize=7, fontweight="bold", color=color,
        bbox=dict(boxstyle="round,pad=0.2", facecolor="white", edgecolor=color, alpha=0.8)
    )

if btc_prices:
    ax2.legend(loc="upper left", fontsize=9)
ax2.yaxis.set_major_formatter(mticker.FuncFormatter(lambda x, p: f"${x:,.0f}"))

# ── Panel 3: CLOB UP/DOWN Ask Prices ────────────────────────────────────
ax3 = axes[2]
fmt_ax(ax3, "CLOB Market Prices (Ask)", "Price")

if clob_up:
    cup_times = [ts_to_dt(r["timestamp"]) for r in clob_up]
    cup_ask   = [r["ask"] for r in clob_up]
    ax3.plot(cup_times, cup_ask, color="#4CAF50", linewidth=0.8, alpha=0.7, label="UP ask")

if clob_down:
    cdn_times = [ts_to_dt(r["timestamp"]) for r in clob_down]
    cdn_ask   = [r["ask"] for r in clob_down]
    ax3.plot(cdn_times, cdn_ask, color="#F44336", linewidth=0.8, alpha=0.7, label="DOWN ask")

ax3.axhline(0.5, color="gray", linewidth=0.5, linestyle=":", alpha=0.5)
ax3.set_ylim(0, 1)
if clob_up or clob_down:
    ax3.legend(loc="upper left", fontsize=9)

# ── Panel 4: Trade P&L Waterfall ─────────────────────────────────────────
ax4 = axes[3]
fmt_ax(ax4, "Trade P&L (2% fee tier)", "P&L (USD)")

trade_pnls = []
trade_labels = []
trade_colors = []
if trades:
    for t in trades:
        if t["pnl_2pct"] is None:
            continue
        trade_labels.append(f"#{t['id']} {t['strategy'][:3].upper()}\n{t['side']}")
        trade_pnls.append(t["pnl_2pct"])
        trade_colors.append("#4CAF50" if t["pnl_2pct"] > 0 else "#F44336")

if trade_pnls:
    bars = ax4.bar(range(len(trade_pnls)), trade_pnls, color=trade_colors,
                   edgecolor="black", linewidth=0.5, width=0.6)
    ax4.set_xticks(range(len(trade_labels)))
    ax4.set_xticklabels(trade_labels, fontsize=8)
    ax4.axhline(0, color="black", linewidth=0.8)

    for i, (bar, pnl) in enumerate(zip(bars, trade_pnls)):
        ax4.text(bar.get_x() + bar.get_width()/2, bar.get_height() + (1 if pnl > 0 else -3),
                 f"${pnl:+.1f}", ha="center", va="bottom" if pnl > 0 else "top",
                 fontsize=9, fontweight="bold")

    # Add cumulative line
    cum_pnl = []
    running = 0
    for p in trade_pnls:
        running += p
        cum_pnl.append(running)
    ax4_twin = ax4.twinx()
    ax4_twin.plot(range(len(cum_pnl)), cum_pnl, color="#2196F3", linewidth=2,
                  marker="o", markersize=6, label=f"Cumulative ${running:+.1f}")
    ax4_twin.set_ylabel("Cumulative P&L (USD)", fontsize=10)
    ax4_twin.legend(loc="upper left", fontsize=9)

ax4.yaxis.set_major_formatter(mticker.FormatStrFormatter("$%.0f"))

plt.tight_layout(rect=[0, 0, 1, 0.96])
out_path = OUT_DIR / f"run-{RUN_NAME}-dashboard.png"
plt.savefig(out_path, dpi=150, bbox_inches="tight", facecolor="white")
print(f"Saved: {out_path}")
plt.close(fig)

# ── 3. CHART: Trade Detail ──────────────────────────────────────────────
if trade_pnls:
    fig2, axes2 = plt.subplots(2, 2, figsize=(14, 10))
    fig2.suptitle(f"Run {RUN_NAME} — Trade Details", fontsize=14, fontweight="bold")

    # P&L at different fee tiers
    ax = axes2[0, 0]
    fee_labels = ["0%", "1%", "2%", "3%"]
    fee_cols   = ["pnl_0pct", "pnl_1pct", "pnl_2pct", "pnl_3pct"]
    cumulative_by_fee = {f: 0 for f in fee_cols}
    for t in trades:
        for f in fee_cols:
            if t[f] is not None:
                cumulative_by_fee[f] += t[f]

    fee_totals = [cumulative_by_fee[f] for f in fee_cols]
    colors = ["#4CAF50" if v > 0 else "#F44336" for v in fee_totals]
    bars = ax.bar(fee_labels, fee_totals, color=colors, edgecolor="black", linewidth=0.5)
    for bar, val in zip(bars, fee_totals):
        yoff = 0.5 if val >= 0 else -0.5
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height() + yoff,
                f"${val:+.1f}", ha="center", va="bottom" if val >= 0 else "top",
                fontsize=10, fontweight="bold")
    ax.set_title("Total P&L by Fee Tier", fontweight="bold")
    ax.set_ylabel("P&L (USD)")
    ax.axhline(0, color="black", linewidth=0.5)
    ax.grid(True, alpha=0.3)

    # Strategy breakdown
    ax = axes2[0, 1]
    strat_pnl = {}
    strat_count = {}
    for t in trades:
        if t["pnl_2pct"] is None:
            continue
        s = t["strategy"]
        pnl = t["pnl_2pct"]
        strat_pnl[s] = strat_pnl.get(s, 0) + pnl
        strat_count[s] = strat_count.get(s, 0) + 1

    strats = list(strat_pnl.keys())
    pnls = [strat_pnl[s] for s in strats]
    counts = [strat_count[s] for s in strats]
    colors = ["#4CAF50" if p > 0 else "#F44336" for p in pnls]
    bars = ax.bar(strats, pnls, color=colors, edgecolor="black", linewidth=0.5)
    for bar, val, cnt in zip(bars, pnls, counts):
        yoff = 0.5 if val >= 0 else -0.5
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height() + yoff,
                f"${val:+.1f}\n({cnt} trades)", ha="center",
                va="bottom" if val >= 0 else "top", fontsize=9, fontweight="bold")
    ax.set_title("P&L by Strategy (2% fee)", fontweight="bold")
    ax.set_ylabel("P&L (USD)")
    ax.axhline(0, color="black", linewidth=0.5)
    ax.grid(True, alpha=0.3)

    # Entry prices distribution
    ax = axes2[1, 0]
    entry_prices = [t["entry_price"] for t in trades if t["pnl_2pct"] is not None]
    sides = [t["side"] for t in trades if t["pnl_2pct"] is not None]
    pnl_vals = [t["pnl_2pct"] for t in trades if t["pnl_2pct"] is not None]
    colors_scatter = ["green" if p > 0 else "red" for p in pnl_vals]
    markers = ["^" if s == "UP" else "v" for s in sides]
    resolved_trades = [t for t in trades if t["pnl_2pct"] is not None]
    for i in range(len(resolved_trades)):
        ax.scatter(i, entry_prices[i], color=colors_scatter[i], marker=markers[i],
                   s=200, edgecolors="black", linewidth=0.8, zorder=5)
        ax.annotate(f"#{resolved_trades[i]['id']} {sides[i]}\n${entry_prices[i]:.2f}",
                   (i, entry_prices[i]), textcoords="offset points",
                   xytext=(0, 15), ha="center", fontsize=8)
    ax.set_title("Entry Prices", fontweight="bold")
    ax.set_ylabel("CLOB Ask Price")
    ax.set_xticks(range(len(resolved_trades)))
    ax.set_xticklabels([f"#{t['id']}" for t in resolved_trades], fontsize=9)
    ax.set_ylim(0, 1)
    ax.axhline(0.5, color="gray", linestyle=":", alpha=0.5)
    ax.grid(True, alpha=0.3)

    # Size vs P&L
    ax = axes2[1, 1]
    sizes = [t["size"] for t in resolved_trades]
    for i in range(len(resolved_trades)):
        ax.scatter(sizes[i], pnl_vals[i], color=colors_scatter[i], marker=markers[i],
                   s=200, edgecolors="black", linewidth=0.8, zorder=5)
        ax.annotate(f"#{resolved_trades[i]['id']}", (sizes[i], pnl_vals[i]),
                   textcoords="offset points", xytext=(8, 5), fontsize=9, fontweight="bold")
    ax.set_title("Position Size vs P&L", fontweight="bold")
    ax.set_xlabel("Size (simulated $)")
    ax.set_ylabel("P&L (USD)")
    ax.axhline(0, color="black", linewidth=0.5)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    out_path2 = OUT_DIR / f"run-{RUN_NAME}-trades.png"
    plt.savefig(out_path2, dpi=150, bbox_inches="tight", facecolor="white")
    print(f"Saved: {out_path2}")
    plt.close(fig2)

print("Done!")
