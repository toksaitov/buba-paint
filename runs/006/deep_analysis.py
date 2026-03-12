#!/usr/bin/env python3
"""Deep statistical analysis of run 006."""

import sqlite3
import json
import math
from collections import defaultdict
from scipy import stats
import numpy as np

DB = "/Users/toksaitov/Desktop/buba-paint/runs/006/buba-paint.db"
conn = sqlite3.connect(DB)
conn.row_factory = sqlite3.Row
cur = conn.cursor()

def fmt_pct(x):
    return f"{x*100:.1f}%"

def section(title):
    print(f"\n{'='*80}")
    print(f"  {title}")
    print(f"{'='*80}\n")

# ─── Helper: join signals to trades (within 2s, same strategy+direction) ───
def get_trades_with_signals():
    """Join trades with their matching signals to get confidence and metadata."""
    cur.execute("""
        SELECT t.id as trade_id, t.timestamp as t_ts, t.strategy, t.side, t.entry_price, t.size,
               tr.settlement_price, tr.pnl_0pct,
               s.metadata, s.timestamp as s_ts
        FROM simulated_trades t
        JOIN trade_results tr ON tr.trade_id = t.id
        LEFT JOIN signals s ON s.strategy = t.strategy
            AND s.direction = t.side
            AND ABS(s.timestamp - t.timestamp) <= 2000
        ORDER BY t.id
    """)
    rows = cur.fetchall()
    # Deduplicate: keep closest signal per trade
    seen = {}
    for r in rows:
        tid = r['trade_id']
        if tid not in seen:
            seen[tid] = r
        else:
            # keep the one with smallest time diff
            if r['s_ts'] is not None:
                old = seen[tid]
                if old['s_ts'] is None or abs(r['s_ts'] - r['t_ts']) < abs(old['s_ts'] - old['t_ts']):
                    seen[tid] = r
    return list(seen.values())

all_trades = get_trades_with_signals()

# Parse metadata for each trade
def parse_meta(row):
    if row['metadata']:
        try:
            return json.loads(row['metadata'])
        except:
            pass
    return {}

# Win/loss helper
def is_win(row):
    return row['pnl_0pct'] > 0

# ═══════════════════════════════════════════════════════════════════════════════
# 1. CONFIDENCE VS OUTCOME
# ═══════════════════════════════════════════════════════════════════════════════
section("1. CONFIDENCE VS OUTCOME (latency-arb)")

la_trades = [r for r in all_trades if r['strategy'] == 'latency-arb']

# Bucket definitions
conf_buckets = [
    ("<=0.70",   lambda c: c <= 0.70),
    ("0.71-0.75", lambda c: 0.70 < c <= 0.75),
    ("0.75-0.80", lambda c: 0.75 < c <= 0.80),
    ("0.80-0.90", lambda c: 0.80 < c <= 0.90),
    ("0.90+",     lambda c: c > 0.90),
]

print(f"{'Bucket':<12} {'Count':>6} {'Win%':>8} {'TotalPnL':>12} {'AvgPnL':>10}")
print("-" * 52)

for bname, bfn in conf_buckets:
    meta_trades = []
    for r in la_trades:
        m = parse_meta(r)
        if 'confidence' in m and bfn(m['confidence']):
            meta_trades.append(r)
    if not meta_trades:
        print(f"{bname:<12} {'0':>6} {'N/A':>8} {'N/A':>12} {'N/A':>10}")
        continue
    wins = sum(1 for r in meta_trades if is_win(r))
    total_pnl = sum(r['pnl_0pct'] for r in meta_trades)
    avg_pnl = total_pnl / len(meta_trades)
    wr = wins / len(meta_trades)
    print(f"{bname:<12} {len(meta_trades):>6} {fmt_pct(wr):>8} {total_pnl:>12.2f} {avg_pnl:>10.2f}")

# Also show unmatched
no_signal = [r for r in la_trades if r['metadata'] is None]
print(f"\nLatency-arb trades without signal match: {len(no_signal)}")

# ═══════════════════════════════════════════════════════════════════════════════
# 2. MOMENTUM MAGNITUDE VS OUTCOME
# ═══════════════════════════════════════════════════════════════════════════════
section("2. MOMENTUM MAGNITUDE VS OUTCOME (latency-arb)")

mom_data = []
for r in la_trades:
    m = parse_meta(r)
    if 'momentum' in m:
        mom_data.append((abs(m['momentum']), r))

if mom_data:
    mom_data.sort(key=lambda x: x[0])
    moms = [x[0] for x in mom_data]

    # Fixed bands
    mom_buckets = [
        ("0.0015-0.002", lambda v: 0.0015 <= v < 0.002),
        ("0.002-0.003",  lambda v: 0.002 <= v < 0.003),
        ("0.003-0.005",  lambda v: 0.003 <= v < 0.005),
        ("0.005-0.01",   lambda v: 0.005 <= v < 0.01),
        ("0.01+",        lambda v: v >= 0.01),
    ]

    print(f"Momentum range: {min(moms):.6f} to {max(moms):.6f}")
    print(f"Median: {np.median(moms):.6f}, Mean: {np.mean(moms):.6f}\n")

    print(f"{'Bucket':<14} {'Count':>6} {'Win%':>8} {'TotalPnL':>12} {'AvgPnL':>10}")
    print("-" * 54)

    for bname, bfn in mom_buckets:
        bt = [x[1] for x in mom_data if bfn(x[0])]
        if not bt:
            print(f"{bname:<14} {'0':>6}")
            continue
        wins = sum(1 for r in bt if is_win(r))
        total_pnl = sum(r['pnl_0pct'] for r in bt)
        avg_pnl = total_pnl / len(bt)
        wr = wins / len(bt)
        print(f"{bname:<14} {len(bt):>6} {fmt_pct(wr):>8} {total_pnl:>12.2f} {avg_pnl:>10.2f}")

    # Also quintiles
    print("\n--- Quintile view ---")
    q_edges = np.percentile(moms, [0, 20, 40, 60, 80, 100])
    for i in range(5):
        lo, hi = q_edges[i], q_edges[i+1]
        bt = [x[1] for x in mom_data if lo <= x[0] <= hi] if i == 4 else [x[1] for x in mom_data if lo <= x[0] < hi]
        if not bt:
            continue
        wins = sum(1 for r in bt if is_win(r))
        total_pnl = sum(r['pnl_0pct'] for r in bt)
        avg_pnl = total_pnl / len(bt)
        wr = wins / len(bt)
        print(f"  Q{i+1} [{lo:.5f}-{hi:.5f}]: n={len(bt):>4}, WR={fmt_pct(wr):>7}, PnL={total_pnl:>10.2f}, Avg={avg_pnl:>8.2f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 3. SEQUENTIAL DEPENDENCY
# ═══════════════════════════════════════════════════════════════════════════════
section("3. SEQUENTIAL DEPENDENCY")

# Get all trades ordered by time
cur.execute("""
    SELECT t.id, t.timestamp, tr.pnl_0pct
    FROM simulated_trades t
    JOIN trade_results tr ON tr.trade_id = t.id
    ORDER BY t.timestamp
""")
all_ordered = cur.fetchall()
outcomes = [1 if r['pnl_0pct'] > 0 else 0 for r in all_ordered]

# P(win | prev win) vs P(win | prev loss)
win_after_win = 0
total_after_win = 0
win_after_loss = 0
total_after_loss = 0

for i in range(1, len(outcomes)):
    if outcomes[i-1] == 1:
        total_after_win += 1
        if outcomes[i] == 1:
            win_after_win += 1
    else:
        total_after_loss += 1
        if outcomes[i] == 1:
            win_after_loss += 1

overall_wr = sum(outcomes) / len(outcomes)
print(f"Overall win rate: {fmt_pct(overall_wr)} ({sum(outcomes)}/{len(outcomes)})")
print(f"P(win | prev win):  {fmt_pct(win_after_win/total_after_win) if total_after_win else 'N/A'} ({win_after_win}/{total_after_win})")
print(f"P(win | prev loss): {fmt_pct(win_after_loss/total_after_loss) if total_after_loss else 'N/A'} ({win_after_loss}/{total_after_loss})")

# Chi-squared test
contingency = [
    [win_after_win, total_after_win - win_after_win],
    [win_after_loss, total_after_loss - win_after_loss]
]
chi2, p_val, dof, expected = stats.chi2_contingency(contingency)
print(f"\nChi-squared test:")
print(f"  chi2 = {chi2:.4f}, p-value = {p_val:.4f}, dof = {dof}")
print(f"  {'SIGNIFICANT' if p_val < 0.05 else 'NOT significant'} at p<0.05")

# Autocorrelation
from numpy import corrcoef
if len(outcomes) > 2:
    ac = corrcoef(outcomes[:-1], outcomes[1:])[0, 1]
    print(f"\nLag-1 autocorrelation: {ac:.4f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 4. TIME BETWEEN TRADES
# ═══════════════════════════════════════════════════════════════════════════════
section("4. TIME BETWEEN TRADES")

timestamps = [r['timestamp'] for r in all_ordered]
pnls = [r['pnl_0pct'] for r in all_ordered]
gaps_ms = [timestamps[i] - timestamps[i-1] for i in range(1, len(timestamps))]
gaps_min = [g / 60000 for g in gaps_ms]

print(f"Total trades: {len(timestamps)}")
print(f"Time gap stats (minutes):")
print(f"  Min: {min(gaps_min):.1f}, Max: {max(gaps_min):.1f}")
print(f"  Median: {np.median(gaps_min):.1f}, Mean: {np.mean(gaps_min):.1f}")
print(f"  Std: {np.std(gaps_min):.1f}")

# Distribution
print(f"\nDistribution:")
dist_bins = [(0, 1, "<1min"), (1, 5, "1-5min"), (5, 10, "5-10min"), (10, 30, "10-30min"),
             (30, 60, "30-60min"), (60, 120, "1-2h"), (120, 360, "2-6h"), (360, 99999, "6h+")]
for lo, hi, label in dist_bins:
    c = sum(1 for g in gaps_min if lo <= g < hi)
    if c > 0:
        print(f"  {label:<10}: {c:>4} ({fmt_pct(c/len(gaps_min))})")

# Correlation with performance
gap_buckets = [
    ("<10min",  lambda g: g < 10),
    ("10-30min", lambda g: 10 <= g < 30),
    ("30-60min", lambda g: 30 <= g < 60),
    ("1h+",      lambda g: g >= 60),
]

print(f"\n{'Gap Bucket':<12} {'Count':>6} {'Win%':>8} {'AvgPnL':>10}")
print("-" * 40)
for bname, bfn in gap_buckets:
    indices = [i for i, g in enumerate(gaps_min) if bfn(g)]
    bt = [(outcomes[i+1], pnls[i+1]) for i in indices]
    if not bt:
        continue
    wins = sum(1 for o, _ in bt if o == 1)
    avg_pnl = sum(p for _, p in bt) / len(bt)
    print(f"{bname:<12} {len(bt):>6} {fmt_pct(wins/len(bt)):>8} {avg_pnl:>10.2f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 5. BTC VOLATILITY VS BOT PERFORMANCE
# ═══════════════════════════════════════════════════════════════════════════════
section("5. BTC VOLATILITY VS BOT PERFORMANCE")

vol_data = []
for r in all_ordered:
    ts = r['timestamp']
    # 30-min BTC price range before trade
    cur.execute("""
        SELECT MIN(price), MAX(price), AVG(price)
        FROM tick_data
        WHERE source = 'binance' AND timestamp BETWEEN ? AND ? AND price IS NOT NULL
    """, (ts - 1800000, ts))
    vr = cur.fetchone()
    if vr and vr[0] is not None and vr[2] > 0:
        price_range = (vr[1] - vr[0]) / vr[2]  # range as % of avg price
        vol_data.append((price_range, r['pnl_0pct'], 1 if r['pnl_0pct'] > 0 else 0))

if vol_data:
    vols = [x[0] for x in vol_data]
    print(f"30-min BTC range (% of price):")
    print(f"  Min: {min(vols)*100:.4f}%, Max: {max(vols)*100:.4f}%, Median: {np.median(vols)*100:.4f}%")

    # Terciles
    tercile_edges = np.percentile(vols, [0, 33.3, 66.6, 100])
    labels = ["Low", "Medium", "High"]
    print(f"\n{'Volatility':<10} {'Range%':<20} {'Count':>6} {'Win%':>8} {'AvgPnL':>10} {'TotalPnL':>12}")
    print("-" * 70)
    for i in range(3):
        lo, hi = tercile_edges[i], tercile_edges[i+1]
        bt = [(p, w) for v, p, w in vol_data if (lo <= v <= hi if i == 2 else lo <= v < hi)]
        if not bt:
            continue
        wins = sum(w for _, w in bt)
        total_pnl = sum(p for p, _ in bt)
        avg_pnl = total_pnl / len(bt)
        wr = wins / len(bt)
        print(f"{labels[i]:<10} {lo*100:.4f}-{hi*100:.4f}%  {len(bt):>5} {fmt_pct(wr):>8} {avg_pnl:>10.2f} {total_pnl:>12.2f}")

    # Correlation
    corr, p = stats.pearsonr([x[0] for x in vol_data], [x[1] for x in vol_data])
    print(f"\nPearson correlation (volatility vs PnL): r={corr:.4f}, p={p:.4f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 6. ENTRY PRICE ACCURACY
# ═══════════════════════════════════════════════════════════════════════════════
section("6. ENTRY PRICE ACCURACY")

# For all trades
cur.execute("""
    SELECT t.strategy, t.entry_price, t.size, tr.pnl_0pct, tr.settlement_price
    FROM simulated_trades t
    JOIN trade_results tr ON tr.trade_id = t.id
""")
price_data = cur.fetchall()

for strat in ['latency-arb', 'spread-capture']:
    strades = [r for r in price_data if r['strategy'] == strat]
    wins = [r for r in strades if r['pnl_0pct'] > 0]
    losses = [r for r in strades if r['pnl_0pct'] <= 0]

    print(f"\n--- {strat} ---")
    if wins:
        print(f"  Wins ({len(wins)}):   avg entry={np.mean([r['entry_price'] for r in wins]):.4f}, avg PnL=${np.mean([r['pnl_0pct'] for r in wins]):.2f}")
    if losses:
        print(f"  Losses ({len(losses)}): avg entry={np.mean([r['entry_price'] for r in losses]):.4f}, avg PnL=${np.mean([r['pnl_0pct'] for r in losses]):.2f}")

# Entry price buckets
print("\n--- Entry Price Buckets (all trades) ---")
ep_buckets = [
    ("<0.30",   lambda p: p < 0.30),
    ("0.30-0.40", lambda p: 0.30 <= p < 0.40),
    ("0.40-0.50", lambda p: 0.40 <= p < 0.50),
    ("0.50-0.60", lambda p: 0.50 <= p < 0.60),
    ("0.60-0.70", lambda p: 0.60 <= p < 0.70),
    ("0.70+",     lambda p: p >= 0.70),
]

print(f"{'Bucket':<12} {'Count':>6} {'Win%':>8} {'AvgWin':>10} {'AvgLoss':>10} {'EV':>10}")
print("-" * 60)
for bname, bfn in ep_buckets:
    bt = [r for r in price_data if bfn(r['entry_price'])]
    if not bt:
        continue
    wins = [r for r in bt if r['pnl_0pct'] > 0]
    losses = [r for r in bt if r['pnl_0pct'] <= 0]
    wr = len(wins) / len(bt)
    avg_win = np.mean([r['pnl_0pct'] for r in wins]) if wins else 0
    avg_loss = np.mean([r['pnl_0pct'] for r in losses]) if losses else 0
    ev = wr * avg_win + (1 - wr) * avg_loss
    print(f"{bname:<12} {len(bt):>6} {fmt_pct(wr):>8} {avg_win:>10.2f} {avg_loss:>10.2f} {ev:>10.2f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 7. SPREAD CAPTURE DEEP DIVE
# ═══════════════════════════════════════════════════════════════════════════════
section("7. SPREAD CAPTURE PROFITABILITY DEEP DIVE")

cur.execute("""
    SELECT t.id, t.timestamp, t.market_id, t.side, t.entry_price, t.size,
           tr.pnl_0pct, tr.settlement_price
    FROM simulated_trades t
    JOIN trade_results tr ON tr.trade_id = t.id
    WHERE t.strategy = 'spread-capture'
    ORDER BY t.timestamp
""")
sc_trades = cur.fetchall()

# Group into pairs by timestamp + market_id
pairs = defaultdict(list)
for r in sc_trades:
    key = (r['timestamp'], r['market_id'])
    pairs[key].append(r)

print(f"Total spread-capture trades: {len(sc_trades)}")
print(f"Number of pairs: {len(pairs)}")

pair_pnls = []
print(f"\n{'Pair#':>5} {'UP_entry':>10} {'DN_entry':>10} {'UP_PnL':>10} {'DN_PnL':>10} {'Net':>10}")
print("-" * 58)
for i, (key, trades) in enumerate(sorted(pairs.items())):
    up_t = [t for t in trades if t['side'] == 'UP']
    dn_t = [t for t in trades if t['side'] == 'DOWN']
    up_pnl = sum(t['pnl_0pct'] for t in up_t)
    dn_pnl = sum(t['pnl_0pct'] for t in dn_t)
    net = up_pnl + dn_pnl
    pair_pnls.append(net)
    up_entry = up_t[0]['entry_price'] if up_t else None
    dn_entry = dn_t[0]['entry_price'] if dn_t else None
    print(f"{i+1:>5} {up_entry:>10.4f} {dn_entry:>10.4f} {up_pnl:>10.2f} {dn_pnl:>10.2f} {net:>10.2f}")

if pair_pnls:
    print(f"\nPair summary:")
    print(f"  Total pairs: {len(pair_pnls)}")
    print(f"  Profitable: {sum(1 for p in pair_pnls if p > 0)}")
    print(f"  Unprofitable: {sum(1 for p in pair_pnls if p <= 0)}")
    print(f"  Avg net PnL/pair: ${np.mean(pair_pnls):.2f}")
    print(f"  Total net PnL: ${sum(pair_pnls):.2f}")

# Spread edge correlation
print("\n--- Spread Edge vs Pair PnL ---")
sc_signals = []
for key, trades in sorted(pairs.items()):
    ts = key[0]
    cur.execute("""
        SELECT metadata FROM signals
        WHERE strategy = 'spread-capture' AND ABS(timestamp - ?) <= 2000
        LIMIT 1
    """, (ts,))
    sr = cur.fetchone()
    if sr and sr['metadata']:
        m = json.loads(sr['metadata'])
        edge = m.get('spreadEdge', None)
        net = sum(t['pnl_0pct'] for t in trades)
        if edge is not None:
            sc_signals.append((edge, net))

if sc_signals:
    print(f"{'Edge':>8} {'NetPnL':>10}")
    print("-" * 20)
    for edge, pnl in sorted(sc_signals):
        print(f"{edge:>8.4f} {pnl:>10.2f}")

    if len(sc_signals) > 2:
        corr, p = stats.pearsonr([x[0] for x in sc_signals], [x[1] for x in sc_signals])
        print(f"\nPearson correlation (edge vs PnL): r={corr:.4f}, p={p:.4f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 8. KELLY ACTIVATION TIMELINE
# ═══════════════════════════════════════════════════════════════════════════════
section("8. KELLY ACTIVATION TIMELINE")

for strat in ['latency-arb', 'spread-capture']:
    cur.execute("""
        SELECT t.id, t.timestamp, t.strategy, tr.pnl_0pct
        FROM simulated_trades t
        JOIN trade_results tr ON tr.trade_id = t.id
        WHERE t.strategy = ?
        ORDER BY t.timestamp
    """, (strat,))
    strades = cur.fetchall()

    if len(strades) < 20:
        print(f"\n{strat}: only {len(strades)} trades (Kelly never activated at 20)")
        # Show what we have
        wins_all = sum(1 for r in strades if r['pnl_0pct'] > 0)
        total_pnl = sum(r['pnl_0pct'] for r in strades)
        print(f"  Total trades: {len(strades)}, WR: {fmt_pct(wins_all/len(strades)) if strades else 'N/A'}, PnL: ${total_pnl:.2f}")
        continue

    # Kelly activates at trade 20
    pre_kelly = strades[:20]
    post_kelly = strades[20:]

    pre_wins = sum(1 for r in pre_kelly if r['pnl_0pct'] > 0)
    post_wins = sum(1 for r in post_kelly if r['pnl_0pct'] > 0)
    pre_pnl = sum(r['pnl_0pct'] for r in pre_kelly)
    post_pnl = sum(r['pnl_0pct'] for r in post_kelly)

    # Time of activation
    activation_ts = strades[19]['timestamp']
    start_ts = strades[0]['timestamp']
    hours_to_activate = (activation_ts - start_ts) / 3600000

    print(f"\n--- {strat} ---")
    print(f"  Kelly activated after trade #{20} at +{hours_to_activate:.1f}h")
    print(f"  WR at activation: {fmt_pct(pre_wins/20)}")
    print(f"  Pre-Kelly  (1-20):    WR={fmt_pct(pre_wins/20):<8} PnL=${pre_pnl:>10.2f}  AvgPnL=${pre_pnl/20:>8.2f}")
    print(f"  Post-Kelly (21-{len(strades)}): WR={fmt_pct(post_wins/len(post_kelly)):<8} PnL=${post_pnl:>10.2f}  AvgPnL=${post_pnl/len(post_kelly):>8.2f}")

# ═══════════════════════════════════════════════════════════════════════════════
# 9. DRAWDOWN RECOVERY ANALYSIS
# ═══════════════════════════════════════════════════════════════════════════════
section("9. DRAWDOWN RECOVERY ANALYSIS")

cur.execute("SELECT timestamp, balance FROM balance_log ORDER BY timestamp")
bal_log = cur.fetchall()

# Track drawdowns
peak = 0
peak_ts = 0
drawdowns = []  # (peak_bal, peak_ts, trough_bal, trough_ts, recovery_ts, depth%)
in_drawdown = False
current_trough = None
current_trough_ts = None
current_peak = None
current_peak_ts = None

for r in bal_log:
    bal = r['balance']
    ts = r['timestamp']

    if bal >= peak:
        if in_drawdown and current_trough is not None:
            # Recovered
            depth = (current_peak - current_trough) / current_peak
            if depth >= 0.05:  # 5% threshold
                drawdowns.append((current_peak, current_peak_ts, current_trough, current_trough_ts, ts, depth))
            in_drawdown = False
        peak = bal
        peak_ts = ts
        current_peak = bal
        current_peak_ts = ts
        current_trough = bal
        current_trough_ts = ts
    else:
        in_drawdown = True
        if bal < current_trough:
            current_trough = bal
            current_trough_ts = ts

# Check if still in drawdown
if in_drawdown and current_trough is not None:
    depth = (current_peak - current_trough) / current_peak
    if depth >= 0.05:
        drawdowns.append((current_peak, current_peak_ts, current_trough, current_trough_ts, None, depth))

if drawdowns:
    print(f"Significant drawdowns (>5%):\n")
    print(f"{'#':>3} {'Peak$':>10} {'Trough$':>10} {'Depth':>8} {'Recovery':>12}")
    print("-" * 50)
    for i, (pk, pk_ts, tr, tr_ts, rec_ts, depth) in enumerate(drawdowns):
        if rec_ts:
            rec_hours = (rec_ts - pk_ts) / 3600000
            rec_str = f"{rec_hours:.1f}h"
        else:
            rec_str = "ONGOING"
        print(f"{i+1:>3} {pk:>10.2f} {tr:>10.2f} {fmt_pct(depth):>8} {rec_str:>12}")
else:
    print("No drawdowns >5% found.")

# Max drawdown
print(f"\n--- Running Drawdown ---")
peak_running = 0
max_dd = 0
max_dd_peak = 0
max_dd_trough = 0
for r in bal_log:
    bal = r['balance']
    if bal > peak_running:
        peak_running = bal
    dd = (peak_running - bal) / peak_running if peak_running > 0 else 0
    if dd > max_dd:
        max_dd = dd
        max_dd_peak = peak_running
        max_dd_trough = bal

print(f"  Max drawdown: {fmt_pct(max_dd)} (${max_dd_peak:.2f} -> ${max_dd_trough:.2f})")

# ═══════════════════════════════════════════════════════════════════════════════
# 10. WIN/LOSS CLUSTERING (RUNS TEST)
# ═══════════════════════════════════════════════════════════════════════════════
section("10. WIN/LOSS CLUSTERING (RUNS TEST)")

# Count runs
n_wins = sum(outcomes)
n_losses = len(outcomes) - n_wins
n = len(outcomes)

runs = 1
for i in range(1, len(outcomes)):
    if outcomes[i] != outcomes[i-1]:
        runs += 1

# Expected runs under independence
expected_runs = 1 + (2 * n_wins * n_losses) / n
var_runs = (2 * n_wins * n_losses * (2 * n_wins * n_losses - n)) / (n**2 * (n - 1))
std_runs = math.sqrt(var_runs) if var_runs > 0 else 0

z_score = (runs - expected_runs) / std_runs if std_runs > 0 else 0
p_value = 2 * (1 - stats.norm.cdf(abs(z_score)))  # two-tailed

print(f"Total trades: {n}")
print(f"Wins: {n_wins}, Losses: {n_losses}")
print(f"Observed runs: {runs}")
print(f"Expected runs (random): {expected_runs:.1f}")
print(f"Std of runs: {std_runs:.2f}")
print(f"Z-score: {z_score:.4f}")
print(f"P-value (two-tailed): {p_value:.4f}")
print(f"{'CLUSTERED (fewer runs than expected)' if z_score < -1.96 else 'DISPERSED (more runs than expected)' if z_score > 1.96 else 'RANDOM (no significant clustering)'}")

# Show longest streaks
max_win_streak = 0
max_loss_streak = 0
cur_streak = 1
for i in range(1, len(outcomes)):
    if outcomes[i] == outcomes[i-1]:
        cur_streak += 1
    else:
        if outcomes[i-1] == 1:
            max_win_streak = max(max_win_streak, cur_streak)
        else:
            max_loss_streak = max(max_loss_streak, cur_streak)
        cur_streak = 1
# Final streak
if outcomes[-1] == 1:
    max_win_streak = max(max_win_streak, cur_streak)
else:
    max_loss_streak = max(max_loss_streak, cur_streak)

print(f"\nLongest win streak: {max_win_streak}")
print(f"Longest loss streak: {max_loss_streak}")

# Streak distribution
streaks = []
cur_streak = 1
for i in range(1, len(outcomes)):
    if outcomes[i] == outcomes[i-1]:
        cur_streak += 1
    else:
        streaks.append((outcomes[i-1], cur_streak))
        cur_streak = 1
streaks.append((outcomes[-1], cur_streak))

win_streaks = [l for t, l in streaks if t == 1]
loss_streaks = [l for t, l in streaks if t == 0]

print(f"\nWin streak distribution:  {dict(sorted(defaultdict(int, {l: win_streaks.count(l) for l in set(win_streaks)}).items()))}")
print(f"Loss streak distribution: {dict(sorted(defaultdict(int, {l: loss_streaks.count(l) for l in set(loss_streaks)}).items()))}")

# ═══════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════════════════════════════════════════
section("RUN 006 SUMMARY")

total_pnl = sum(r['pnl_0pct'] for r in all_ordered)
final_bal = bal_log[-1]['balance'] if bal_log else 0
duration_h = (all_ordered[-1]['timestamp'] - all_ordered[0]['timestamp']) / 3600000
print(f"Duration: {duration_h:.1f} hours ({duration_h/24:.1f} days)")
print(f"Total trades: {len(all_ordered)}")
print(f"Overall win rate: {fmt_pct(sum(outcomes)/len(outcomes))}")
print(f"Starting balance: $200.00")
print(f"Final balance: ${final_bal:.2f}")
print(f"Total PnL (0% fee): ${total_pnl:.2f}")
print(f"Return: {fmt_pct((final_bal - 200) / 200)}")
print(f"Max drawdown: {fmt_pct(max_dd)}")

conn.close()
