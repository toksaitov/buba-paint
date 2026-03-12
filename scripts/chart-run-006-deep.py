#!/usr/bin/env python3
"""Deep analysis charts for buba-paint run 006."""

import sqlite3
import json
import os
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
from datetime import datetime, timedelta
from scipy import stats

DB_PATH = os.path.join(os.path.dirname(__file__), '..', 'runs', '006', 'buba-paint.db')
OUT_DIR = os.path.join(os.path.dirname(__file__), '..', 'runs', '006')

conn = sqlite3.connect(DB_PATH)
conn.row_factory = sqlite3.Row

# ─── Load all trades with results ───
trades = conn.execute("""
    SELECT t.id, t.timestamp, t.strategy, t.side, t.entry_price, t.size,
           r.pnl_2pct, r.settlement_price, r.resolved_at
    FROM simulated_trades t
    JOIN trade_results r ON r.trade_id = t.id
    WHERE t.status = 'closed'
    ORDER BY t.timestamp
""").fetchall()

# ─── Load signals ───
signals = conn.execute("""
    SELECT id, timestamp, strategy, direction, metadata
    FROM signals ORDER BY timestamp
""").fetchall()

# ─── Load balance log ───
balance_log = conn.execute("""
    SELECT timestamp, event, trade_id, amount, balance
    FROM balance_log ORDER BY timestamp
""").fetchall()

print(f"Loaded {len(trades)} trades, {len(signals)} signals, {len(balance_log)} balance entries")

# ─── Join signals to trades (within 2s, same strategy+direction) ───
def join_signals_to_trades(trades, signals):
    """Match each trade to the closest signal within 2 seconds."""
    result = []
    for t in trades:
        best_sig = None
        best_dt = float('inf')
        for s in signals:
            if s['strategy'] != t['strategy']:
                continue
            if s['direction'] != t['side']:
                continue
            dt = abs(t['timestamp'] - s['timestamp'])
            if dt <= 2000 and dt < best_dt:
                best_dt = dt
                best_sig = s
        if best_sig and best_sig['metadata']:
            meta = json.loads(best_sig['metadata'])
            conf = meta.get('confidence')
            if conf is not None:
                result.append({
                    'trade_id': t['id'],
                    'timestamp': t['timestamp'],
                    'strategy': t['strategy'],
                    'side': t['side'],
                    'pnl': t['pnl_2pct'],
                    'confidence': conf,
                    'size': t['size'],
                    'entry_price': t['entry_price'],
                })
    return result

joined = join_signals_to_trades(trades, signals)
print(f"Joined {len(joined)} trades to signals")

# ═══════════════════════════════════════════════════════
# FIGURE 1
# ═══════════════════════════════════════════════════════
fig1, axes1 = plt.subplots(2, 2, figsize=(20, 14))
fig1.suptitle('Run 006 Deep Analysis (1/2)', fontsize=16, fontweight='bold', y=0.98)

# ─── Panel 1: Confidence vs Win Rate & EV (latency-arb) ───
ax1 = axes1[0, 0]
la_joined = [j for j in joined if j['strategy'] == 'latency-arb']

buckets = {
    '<=0.70': (0, 0.705),
    '0.71-0.75': (0.705, 0.755),
    '0.76-0.80': (0.755, 0.805),
    '0.81-0.90': (0.805, 0.905),
    '0.90+': (0.905, 2.0),
}

bucket_names = list(buckets.keys())
bucket_wr = []
bucket_ev = []
bucket_count = []

for name, (lo, hi) in buckets.items():
    subset = [j for j in la_joined if lo <= j['confidence'] < hi]
    n = len(subset)
    bucket_count.append(n)
    if n == 0:
        bucket_wr.append(0)
        bucket_ev.append(0)
        continue
    wins = [j for j in subset if j['pnl'] > 0]
    losses = [j for j in subset if j['pnl'] <= 0]
    wr = len(wins) / n
    avg_win = np.mean([j['pnl'] for j in wins]) if wins else 0
    avg_loss = np.mean([j['pnl'] for j in losses]) if losses else 0
    ev = wr * avg_win + (1 - wr) * avg_loss
    bucket_wr.append(wr)
    bucket_ev.append(ev)

x_pos = np.arange(len(bucket_names))
bars = ax1.bar(x_pos, bucket_wr, color='steelblue', alpha=0.8, width=0.6, label='Win Rate')
ax1.set_ylabel('Win Rate', color='steelblue')
ax1.set_ylim(0, 1.1)
ax1.set_xticks(x_pos)
ax1.set_xticklabels(bucket_names, fontsize=9)

# trade count above bars
for i, (bar, cnt) in enumerate(zip(bars, bucket_count)):
    ax1.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.02,
             f'n={cnt}', ha='center', va='bottom', fontsize=8, fontweight='bold')

ax1r = ax1.twinx()
ax1r.plot(x_pos, bucket_ev, 'ro-', linewidth=2, markersize=8, label='EV per trade')
ax1r.set_ylabel('Expected Value ($)', color='red')
ax1r.axhline(0, color='gray', linestyle='--', alpha=0.5)

ax1.set_title('Confidence vs Win Rate & Expected Value (latency-arb)', fontweight='bold')
lines1, labels1 = ax1.get_legend_handles_labels()
lines2, labels2 = ax1r.get_legend_handles_labels()
ax1.legend(lines1 + lines2, labels1 + labels2, loc='upper left', fontsize=8)

# ─── Panel 2: Trade Frequency vs Rolling Win Rate ───
ax2 = axes1[0, 1]

trade_times_ms = [t['timestamp'] for t in trades]
trade_wins = [1 if t['pnl_2pct'] > 0 else 0 for t in trades]
trade_dts = [datetime.utcfromtimestamp(ts / 1000) for ts in trade_times_ms]

if len(trades) >= 10:
    # Rolling 3-hour trade count
    three_hours_ms = 3 * 3600 * 1000
    rolling_3h_count = []
    for i, ts in enumerate(trade_times_ms):
        count = sum(1 for t in trade_times_ms if ts - three_hours_ms <= t <= ts)
        rolling_3h_count.append(count)

    # Rolling 10-trade win rate
    rolling_wr = []
    for i in range(len(trade_wins)):
        start = max(0, i - 9)
        window = trade_wins[start:i+1]
        rolling_wr.append(sum(window) / len(window))

    ax2.plot(trade_dts, rolling_3h_count, 'b-', alpha=0.7, linewidth=1.5, label='3h Trade Count')
    ax2.set_ylabel('3-Hour Trade Count', color='steelblue')
    ax2.set_xlabel('Time')

    ax2r = ax2.twinx()
    ax2r.plot(trade_dts, rolling_wr, 'r-', alpha=0.7, linewidth=1.5, label='Rolling 10-Trade WR')
    ax2r.set_ylabel('Win Rate (rolling 10)', color='red')
    ax2r.set_ylim(0, 1.1)
    ax2r.axhline(0.5, color='gray', linestyle='--', alpha=0.4)

    lines1, labels1 = ax2.get_legend_handles_labels()
    lines2, labels2 = ax2r.get_legend_handles_labels()
    ax2.legend(lines1 + lines2, labels1 + labels2, loc='upper left', fontsize=8)
else:
    ax2.text(0.5, 0.5, 'Not enough trades', ha='center', va='center', transform=ax2.transAxes)

ax2.xaxis.set_major_formatter(mdates.DateFormatter('%m/%d %H:%M'))
ax2.tick_params(axis='x', rotation=30)
ax2.set_title('Trade Frequency vs Rolling Win Rate', fontweight='bold')

# ─── Panel 3: BTC 30-min Volatility vs Outcome ───
ax3 = axes1[1, 0]

vol_data = []
for t in trades:
    ts = t['timestamp']
    thirty_min_ms = 30 * 60 * 1000
    rows = conn.execute("""
        SELECT MIN(price) as lo, MAX(price) as hi, AVG(price) as avg_p
        FROM tick_data
        WHERE source = 'binance' AND timestamp BETWEEN ? AND ? AND price IS NOT NULL
    """, (ts - thirty_min_ms, ts)).fetchone()
    if rows['lo'] and rows['hi'] and rows['avg_p'] > 0:
        vol_range = (rows['hi'] - rows['lo']) / rows['avg_p'] * 100  # percentage
        vol_data.append({
            'vol': vol_range,
            'pnl': t['pnl_2pct'],
            'win': 1 if t['pnl_2pct'] > 0 else 0,
        })

if vol_data:
    vols = [v['vol'] for v in vol_data]
    terciles = np.percentile(vols, [33.3, 66.6])
    vol_buckets = {'Low': [], 'Medium': [], 'High': []}
    for v in vol_data:
        if v['vol'] <= terciles[0]:
            vol_buckets['Low'].append(v)
        elif v['vol'] <= terciles[1]:
            vol_buckets['Medium'].append(v)
        else:
            vol_buckets['High'].append(v)

    bnames = list(vol_buckets.keys())
    bwr = [np.mean([v['win'] for v in vol_buckets[b]]) if vol_buckets[b] else 0 for b in bnames]
    bpnl = [np.sum([v['pnl'] for v in vol_buckets[b]]) if vol_buckets[b] else 0 for b in bnames]
    bcnt = [len(vol_buckets[b]) for b in bnames]

    x_pos3 = np.arange(len(bnames))
    bars3 = ax3.bar(x_pos3 - 0.15, bwr, width=0.3, color='steelblue', alpha=0.8, label='Win Rate')
    ax3.set_ylabel('Win Rate', color='steelblue')
    ax3.set_ylim(0, 1.1)

    ax3r = ax3.twinx()
    bars3b = ax3r.bar(x_pos3 + 0.15, bpnl, width=0.3, color='orange', alpha=0.8, label='Total PnL ($)')
    ax3r.set_ylabel('Total PnL ($)', color='orange')

    for i, cnt in enumerate(bcnt):
        ax3.text(x_pos3[i], max(bwr) + 0.05 if bwr else 0.5,
                 f'n={cnt}', ha='center', fontsize=9, fontweight='bold')

    ax3.set_xticks(x_pos3)
    vol_labels = [f'{b}\n(<={terciles[0]:.2f}%)' if b == 'Low' else
                  f'{b}\n({terciles[0]:.2f}-{terciles[1]:.2f}%)' if b == 'Medium' else
                  f'{b}\n(>{terciles[1]:.2f}%)' for b in bnames]
    ax3.set_xticklabels(vol_labels, fontsize=9)

    lines1, labels1 = ax3.get_legend_handles_labels()
    lines2, labels2 = ax3r.get_legend_handles_labels()
    ax3.legend(lines1 + lines2, labels1 + labels2, loc='upper left', fontsize=8)
else:
    ax3.text(0.5, 0.5, 'No volatility data', ha='center', va='center', transform=ax3.transAxes)

ax3.set_title('Pre-Trade BTC Volatility vs Outcome', fontweight='bold')

# ─── Panel 4: Drawdown Episodes & Recovery ───
ax4 = axes1[1, 1]

bl_times = [datetime.utcfromtimestamp(b['timestamp'] / 1000) for b in balance_log]
bl_balances = [b['balance'] for b in balance_log]

ax4.plot(bl_times, bl_balances, 'b-', linewidth=1.5, label='Equity')

# Compute running peak and drawdowns
peaks = []
running_peak = 0
for b in bl_balances:
    running_peak = max(running_peak, b)
    peaks.append(running_peak)

dd_pcts = [(b - p) / p * 100 if p > 0 else 0 for b, p in zip(bl_balances, peaks)]

# Find drawdown episodes > 10%
in_dd = False
dd_start = None
dd_depth = 0
episodes = []
for i, dd in enumerate(dd_pcts):
    if dd < -10:
        if not in_dd:
            dd_start = i
            in_dd = True
        dd_depth = min(dd_depth, dd)
    else:
        if in_dd:
            # Find recovery point
            episodes.append((dd_start, i, dd_depth))
            in_dd = False
            dd_depth = 0

if in_dd:
    episodes.append((dd_start, len(dd_pcts) - 1, dd_depth))

for start_i, end_i, depth in episodes:
    ax4.axvspan(bl_times[start_i], bl_times[min(end_i, len(bl_times)-1)],
                alpha=0.2, color='red')
    # Recovery point
    if end_i < len(bl_times):
        ax4.plot(bl_times[end_i], bl_balances[end_i], 'g^', markersize=10, zorder=5)
    # Annotate depth
    mid_i = (start_i + end_i) // 2
    ax4.annotate(f'{depth:.1f}%',
                 xy=(bl_times[mid_i], min(bl_balances[start_i:end_i+1])),
                 fontsize=8, fontweight='bold', color='red',
                 ha='center', va='top')

ax4.xaxis.set_major_formatter(mdates.DateFormatter('%m/%d %H:%M'))
ax4.tick_params(axis='x', rotation=30)
ax4.set_ylabel('Balance ($)')
ax4.set_title('Drawdown Episodes & Recovery', fontweight='bold')
ax4.legend(loc='upper left', fontsize=8)

fig1.tight_layout(rect=[0, 0, 1, 0.96])
out1 = os.path.join(OUT_DIR, 'run-006-deep1.png')
fig1.savefig(out1, dpi=150, bbox_inches='tight')
print(f"Saved {out1}")
plt.close(fig1)

# ═══════════════════════════════════════════════════════
# FIGURE 2
# ═══════════════════════════════════════════════════════
fig2, axes2 = plt.subplots(2, 2, figsize=(20, 14))
fig2.suptitle('Run 006 Deep Analysis (2/2)', fontsize=16, fontweight='bold', y=0.98)

# ─── Panel 1: Kelly Activation Impact (latency-arb) ───
ax5 = axes2[0, 0]

la_trades = [t for t in trades if t['strategy'] == 'latency-arb']
la_pnls = [t['pnl_2pct'] for t in la_trades]
la_cum_pnl = np.cumsum(la_pnls)
trade_nums = np.arange(1, len(la_trades) + 1)

ax5.plot(trade_nums, la_cum_pnl, 'b-', linewidth=2, label='Cumulative PnL')
ax5.axhline(0, color='gray', linestyle='--', alpha=0.4)

kelly_trade = 20
if len(la_trades) >= kelly_trade:
    ax5.axvline(kelly_trade, color='red', linestyle='--', linewidth=2, alpha=0.8)
    ax5.annotate('Kelly ON', xy=(kelly_trade, la_cum_pnl[kelly_trade-1]),
                 xytext=(kelly_trade + 5, la_cum_pnl[kelly_trade-1] + 20),
                 fontsize=11, fontweight='bold', color='red',
                 arrowprops=dict(arrowstyle='->', color='red'))

    # Win rates before/after
    pre_wins = sum(1 for t in la_trades[:kelly_trade] if t['pnl_2pct'] > 0)
    pre_wr = pre_wins / kelly_trade
    post_n = len(la_trades) - kelly_trade
    post_wins = sum(1 for t in la_trades[kelly_trade:] if t['pnl_2pct'] > 0)
    post_wr = post_wins / post_n if post_n > 0 else 0

    textstr = f'Pre-Kelly WR: {pre_wr:.1%} ({kelly_trade} trades)\nPost-Kelly WR: {post_wr:.1%} ({post_n} trades)'
    ax5.text(0.02, 0.98, textstr, transform=ax5.transAxes, fontsize=9,
             verticalalignment='top', bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.8))

ax5.set_xlabel('Trade Number')
ax5.set_ylabel('Cumulative PnL ($)')
ax5.set_title('Kelly Criterion Activation: Before vs After (latency-arb)', fontweight='bold')
ax5.legend(loc='lower right', fontsize=9)

# ─── Panel 2: Sequential Trade Dependency ───
ax6 = axes2[0, 1]

all_wins = [1 if t['pnl_2pct'] > 0 else 0 for t in trades]
overall_wr = np.mean(all_wins) if all_wins else 0

win_after_win = []
win_after_loss = []
for i in range(1, len(all_wins)):
    if all_wins[i-1] == 1:
        win_after_win.append(all_wins[i])
    else:
        win_after_loss.append(all_wins[i])

p_win_after_win = np.mean(win_after_win) if win_after_win else 0
p_win_after_loss = np.mean(win_after_loss) if win_after_loss else 0
n_waw = len(win_after_win)
n_wal = len(win_after_loss)

# Binomial CI (95%)
def binom_ci(p, n, z=1.96):
    if n == 0:
        return 0
    return z * np.sqrt(p * (1 - p) / n)

ci_waw = binom_ci(p_win_after_win, n_waw)
ci_wal = binom_ci(p_win_after_loss, n_wal)

x_pos6 = [0, 1]
heights = [p_win_after_win, p_win_after_loss]
errs = [ci_waw, ci_wal]
colors = ['#4CAF50', '#F44336']
labels_6 = [f'P(Win|Prev Win)\nn={n_waw}', f'P(Win|Prev Loss)\nn={n_wal}']

bars6 = ax6.bar(x_pos6, heights, yerr=errs, capsize=10, color=colors, alpha=0.8, width=0.5)
ax6.axhline(overall_wr, color='blue', linestyle='--', linewidth=2, label=f'Overall WR: {overall_wr:.1%}')
ax6.set_xticks(x_pos6)
ax6.set_xticklabels(labels_6, fontsize=10)
ax6.set_ylabel('Win Probability')
ax6.set_ylim(0, 1.1)

for i, (h, e) in enumerate(zip(heights, errs)):
    ax6.text(x_pos6[i], h + e + 0.03, f'{h:.1%}', ha='center', fontsize=11, fontweight='bold')

ax6.legend(loc='upper right', fontsize=9)
ax6.set_title('Does Previous Trade Predict Next?', fontweight='bold')

# ─── Panel 3: Trade P&L Distribution ───
ax7 = axes2[1, 0]

pnls = [t['pnl_2pct'] for t in trades]
wins_pnl = [p for p in pnls if p > 0]
losses_pnl = [p for p in pnls if p <= 0]

bins = np.linspace(min(pnls), max(pnls), 41)
ax7.hist(wins_pnl, bins=bins, color='green', alpha=0.7, label=f'Wins (n={len(wins_pnl)})', edgecolor='darkgreen')
ax7.hist(losses_pnl, bins=bins, color='red', alpha=0.7, label=f'Losses (n={len(losses_pnl)})', edgecolor='darkred')

mean_pnl = np.mean(pnls)
median_pnl = np.median(pnls)
ax7.axvline(mean_pnl, color='blue', linestyle='-', linewidth=2, label=f'Mean: ${mean_pnl:.2f}')
ax7.axvline(median_pnl, color='orange', linestyle='--', linewidth=2, label=f'Median: ${median_pnl:.2f}')

# Normal distribution overlay
mu, sigma = np.mean(pnls), np.std(pnls)
x_norm = np.linspace(min(pnls), max(pnls), 200)
bin_width = bins[1] - bins[0]
y_norm = stats.norm.pdf(x_norm, mu, sigma) * len(pnls) * bin_width
ax7.plot(x_norm, y_norm, 'k--', linewidth=1.5, alpha=0.6, label='Normal fit')

ax7.set_xlabel('Trade PnL ($)')
ax7.set_ylabel('Count')
ax7.legend(loc='upper right', fontsize=8)
ax7.set_title('Trade P&L Distribution', fontweight='bold')

# ─── Panel 4: Cumulative P&L by Side (UP vs DOWN) for latency-arb ───
ax8 = axes2[1, 1]

la_up = [(t['timestamp'], t['pnl_2pct']) for t in la_trades if t['side'] == 'UP']
la_down = [(t['timestamp'], t['pnl_2pct']) for t in la_trades if t['side'] == 'DOWN']

if la_up:
    up_times = [datetime.utcfromtimestamp(ts / 1000) for ts, _ in la_up]
    up_cum = np.cumsum([p for _, p in la_up])
    ax8.plot(up_times, up_cum, 'g-', linewidth=2, label=f'UP ({len(la_up)} trades, ${up_cum[-1]:.1f})')

if la_down:
    dn_times = [datetime.utcfromtimestamp(ts / 1000) for ts, _ in la_down]
    dn_cum = np.cumsum([p for _, p in la_down])
    ax8.plot(dn_times, dn_cum, 'r-', linewidth=2, label=f'DOWN ({len(la_down)} trades, ${dn_cum[-1]:.1f})')

ax8.axhline(0, color='gray', linestyle='--', alpha=0.4)
ax8.xaxis.set_major_formatter(mdates.DateFormatter('%m/%d %H:%M'))
ax8.tick_params(axis='x', rotation=30)
ax8.set_ylabel('Cumulative PnL ($)')
ax8.set_xlabel('Time')
ax8.legend(loc='best', fontsize=9)
ax8.set_title('Alpha Source: UP vs DOWN Trades (latency-arb)', fontweight='bold')

fig2.tight_layout(rect=[0, 0, 1, 0.96])
out2 = os.path.join(OUT_DIR, 'run-006-deep2.png')
fig2.savefig(out2, dpi=150, bbox_inches='tight')
print(f"Saved {out2}")
plt.close(fig2)

conn.close()
print("Done!")
