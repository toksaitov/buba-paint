#!/usr/bin/env python3
import csv
import json
import math
import re
import sqlite3
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RUN_DIR = ROOT / "runs/012/server-20260424-183503"
OUT_DIR = ROOT / "data/experiments/run-012-forensics-001"
DB_PATH = RUN_DIR / "paint.db"
BASELINE_DB = OUT_DIR / "baseline_exact.db"


def dt(ms):
    if ms is None:
        return "n/a"
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")


def usd(value):
    if value is None:
        return "n/a"
    return f"${value:,.2f}"


def pct(value):
    if value is None:
        return "n/a"
    return f"{value * 100:.2f}%"


def connect(path):
    uri = f"file:{path}?mode=ro"
    return sqlite3.connect(uri, uri=True)


def one(conn, query, params=()):
    row = conn.execute(query, params).fetchone()
    if row is None:
        return None
    return row[0] if len(row) == 1 else row


def rows(conn, query, params=()):
    return conn.execute(query, params).fetchall()


def write_csv(path, headers, data):
    with path.open("w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(headers)
        writer.writerows(data)


def balance_timeline(conn):
    data = rows(
        conn,
        """
        select timestamp, event, trade_id, amount, balance
        from balance_log
        order by timestamp, id
        """,
    )
    timeline = []
    hwm = None
    max_dd = 0.0
    hwm_ts = None
    breach = None
    for ts, event, trade_id, amount, balance in data:
        if hwm is None or balance > hwm:
            hwm = balance
            hwm_ts = ts
        drawdown = 0.0 if not hwm else (hwm - balance) / hwm
        max_dd = max(max_dd, drawdown)
        if breach is None and drawdown >= 0.50:
            breach = (ts, balance, hwm, drawdown, event, trade_id)
        timeline.append((ts, dt(ts), event, trade_id, amount, balance, hwm, drawdown))
    return timeline, hwm, hwm_ts, max_dd, breach


def strategy_attribution(conn):
    return rows(
        conn,
        """
        select
            t.strategy,
            count(*) as trades,
            sum(case when r.pnl_net > 0 then 1 else 0 end) as wins,
            sum(case when r.pnl_net <= 0 then 1 else 0 end) as losses,
            sum(r.pnl_net) as pnl_net,
            avg(r.pnl_net) as avg_pnl_net,
            min(r.pnl_net) as worst_trade,
            max(r.pnl_net) as best_trade,
            sum(r.fee_amount) as fees,
            avg(t.size) as avg_size,
            sum(t.size) as total_size
        from simulated_trades t
        join trade_results r on r.trade_id = t.id
        group by t.strategy
        order by pnl_net desc
        """,
    )


def side_attribution(conn):
    return rows(
        conn,
        """
        select
            t.strategy,
            t.side,
            count(*) as trades,
            sum(case when r.pnl_net > 0 then 1 else 0 end) as wins,
            sum(case when r.pnl_net <= 0 then 1 else 0 end) as losses,
            sum(r.pnl_net) as pnl_net,
            avg(r.pnl_net) as avg_pnl_net
        from simulated_trades t
        join trade_results r on r.trade_id = t.id
        group by t.strategy, t.side
        order by pnl_net desc
        """,
    )


def market_attribution(conn):
    return rows(
        conn,
        """
        select
            t.market_id,
            coalesce(m.question, '') as question,
            min(t.timestamp) as first_trade_ms,
            max(t.timestamp) as last_trade_ms,
            count(*) as trades,
            sum(r.pnl_net) as pnl_net,
            min(r.pnl_net) as worst_trade,
            max(r.pnl_net) as best_trade
        from simulated_trades t
        join trade_results r on r.trade_id = t.id
        left join markets m on m.market_id = t.market_id
        group by t.market_id
        order by pnl_net asc
        """,
    )


def trade_rows(conn):
    return rows(
        conn,
        """
        select
            t.id,
            t.timestamp,
            t.market_id,
            coalesce(m.question, '') as question,
            t.strategy,
            t.side,
            t.entry_price,
            t.size,
            t.status,
            t.fill_status,
            t.execution_fidelity,
            r.resolved_at,
            r.settlement_price,
            r.fee_amount,
            r.pnl_net
        from simulated_trades t
        join trade_results r on r.trade_id = t.id
        left join markets m on m.market_id = t.market_id
        order by t.timestamp, t.id
        """,
    )


def signal_status(conn):
    return rows(
        conn,
        """
        select
            coalesce(s.strategy, 'unknown') as strategy,
            coalesce(sm.decision_status, 'missing') as status,
            coalesce(sm.rejection_reason, '') as reason,
            count(*) as count,
            avg(sm.quote_age_ms) as avg_quote_age_ms,
            avg(sm.book_staleness_ms) as avg_book_staleness_ms,
            avg(sm.effective_arrival_delay_ms) as avg_arrival_delay_ms,
            avg(sm.expected_edge) as avg_expected_edge
        from signals s
        left join signal_metrics sm on sm.signal_id = s.id
        group by strategy, status, reason
        order by count desc
        """
    )


def rejection_rollups(conn):
    return rows(
        conn,
        """
        select
            strategy,
            reason,
            sum(count) as total_count,
            count(*) as rollup_rows,
            min(timestamp_ms) as first_ms,
            max(timestamp_ms) as last_ms
        from strategy_rejection_summaries
        group by strategy, reason
        order by total_count desc
        limit 50
        """,
    )


def feed_health(conn):
    return rows(
        conn,
        """
        select
            source,
            event_type,
            count(*) as count,
            min(timestamp_ms) as first_ms,
            max(timestamp_ms) as last_ms
        from feed_health_events
        group by source, event_type
        order by count desc
        """,
    )


def feed_event_counts(conn):
    return rows(
        conn,
        """
        select
            source,
            event_type,
            count(*) as count,
            min(received_at_ms) as first_ms,
            max(received_at_ms) as last_ms
        from feed_events
        group by source, event_type
        order by count desc
        """,
    )


def live_snapshot_summary(conn):
    return rows(
        conn,
        """
        select
            count(*) as snapshots,
            min(timestamp_ms) as first_ms,
            max(timestamp_ms) as last_ms,
            avg(cash_available) as avg_cash_available,
            min(cash_available) as min_cash_available,
            max(cash_available) as max_cash_available,
            avg(total_equity) as avg_total_equity,
            min(total_equity) as min_total_equity,
            max(total_equity) as max_total_equity
        from live_account_snapshots
        """,
    )


def log_counters():
    counters = {}
    for name in ["paint.log", "bot_wrapper.log", "sidecar.log", "agent.log", "dashboard.log"]:
        path = RUN_DIR / name
        text = path.read_text(errors="replace") if path.exists() else ""
        counters[name] = {
            "lines": text.count("\n"),
            "warn": len(re.findall(r"\bWARN\b|warn", text)),
            "error": len(re.findall(r"\bERROR\b|error", text)),
            "feed_disconnect": text.count("feed disconnected"),
            "feed_connected": text.count("feed connected"),
            "strategy_rejection_rollup": text.count("strategy rejection rollup"),
            "paper_execution_rollup": text.count("paper execution rollup"),
            "readonly_rollup": text.count("readonly shadow runtime rollup"),
            "sidecar_user_stream": text.count("user stream"),
            "account_refresh_failed": text.count("account refresh failed"),
            "fatal": len(re.findall(r"fatal|uncaught|unhandled", text, flags=re.I)),
        }
    return counters


def summarize_db(conn):
    timeline, hwm, hwm_ts, max_dd, breach = balance_timeline(conn)
    first_tick, last_tick = one(
        conn,
        "select min(received_at_ms), max(received_at_ms) from feed_events",
    )
    first_signal, last_signal = one(conn, "select min(timestamp), max(timestamp) from signals")
    first_trade, last_trade = one(conn, "select min(timestamp), max(timestamp) from simulated_trades")
    final_balance = timeline[-1][5] if timeline else None
    start_balance = timeline[0][5] if timeline else None
    trade_count = one(conn, "select count(*) from simulated_trades")
    result_count = one(conn, "select count(*) from trade_results")
    wins = one(
        conn,
        """
        select count(*)
        from trade_results
        where pnl_net > 0
        """,
    )
    losses = one(
        conn,
        """
        select count(*)
        from trade_results
        where pnl_net <= 0
        """,
    )
    total_fees = one(conn, "select coalesce(sum(fee_amount), 0) from trade_results")
    total_pnl = one(conn, "select coalesce(sum(pnl_net), 0) from trade_results")
    return {
        "db_path": str(DB_PATH),
        "first_tick_ms": first_tick,
        "last_tick_ms": last_tick,
        "first_tick_utc": dt(first_tick),
        "last_tick_utc": dt(last_tick),
        "first_signal_ms": first_signal,
        "last_signal_ms": last_signal,
        "first_signal_utc": dt(first_signal),
        "last_signal_utc": dt(last_signal),
        "first_trade_ms": first_trade,
        "last_trade_ms": last_trade,
        "first_trade_utc": dt(first_trade),
        "last_trade_utc": dt(last_trade),
        "start_balance": start_balance,
        "final_balance": final_balance,
        "total_pnl": total_pnl,
        "high_water_mark": hwm,
        "high_water_mark_ms": hwm_ts,
        "high_water_mark_utc": dt(hwm_ts),
        "max_drawdown_pct": max_dd,
        "drawdown_breach": {
            "timestamp_ms": breach[0],
            "timestamp_utc": dt(breach[0]),
            "balance": breach[1],
            "high_water_mark": breach[2],
            "drawdown_pct": breach[3],
            "event": breach[4],
            "trade_id": breach[5],
        }
        if breach
        else None,
        "signals": one(conn, "select count(*) from signals"),
        "signal_metrics": one(conn, "select count(*) from signal_metrics"),
        "feed_events": one(conn, "select count(*) from feed_events"),
        "feed_health_events": one(conn, "select count(*) from feed_health_events"),
        "rejection_summary_rows": one(conn, "select count(*) from strategy_rejection_summaries"),
        "trade_count": trade_count,
        "result_count": result_count,
        "wins": wins,
        "losses": losses,
        "win_rate": wins / result_count if result_count else 0.0,
        "total_fees": total_fees,
    }


def baseline_summary():
    if not BASELINE_DB.exists():
        return None
    conn = connect(BASELINE_DB)
    try:
        return summarize_db(conn)
    finally:
        conn.close()


def write_outputs():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    conn = connect(DB_PATH)
    try:
        summary = summarize_db(conn)
        baseline = baseline_summary()
        if baseline:
            summary["baseline_exact"] = {
                key: baseline.get(key)
                for key in [
                    "start_balance",
                    "final_balance",
                    "total_pnl",
                    "high_water_mark",
                    "high_water_mark_utc",
                    "max_drawdown_pct",
                    "first_trade_utc",
                    "last_trade_utc",
                    "trade_count",
                    "result_count",
                    "wins",
                    "losses",
                    "win_rate",
                    "total_fees",
                ]
            }
            summary["baseline_deltas"] = {
                "final_balance": baseline["final_balance"] - summary["final_balance"],
                "trade_count": baseline["trade_count"] - summary["trade_count"],
                "max_drawdown_pp": (baseline["max_drawdown_pct"] - summary["max_drawdown_pct"]) * 100,
            }

        (OUT_DIR / "run_summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

        timeline, _, _, _, _ = balance_timeline(conn)
        write_csv(
            OUT_DIR / "drawdown_timeline.csv",
            ["timestamp_ms", "timestamp_utc", "event", "trade_id", "amount", "balance", "hwm", "drawdown_pct"],
            timeline,
        )

        write_csv(
            OUT_DIR / "strategy_attribution.csv",
            [
                "strategy",
                "trades",
                "wins",
                "losses",
                "pnl_net",
                "avg_pnl_net",
                "worst_trade",
                "best_trade",
                "fees",
                "avg_size",
                "total_size",
            ],
            strategy_attribution(conn),
        )

        write_csv(
            OUT_DIR / "side_attribution.csv",
            ["strategy", "side", "trades", "wins", "losses", "pnl_net", "avg_pnl_net"],
            side_attribution(conn),
        )

        markets = market_attribution(conn)
        write_csv(
            OUT_DIR / "market_attribution.csv",
            ["market_id", "question", "first_trade_ms", "last_trade_ms", "trades", "pnl_net", "worst_trade", "best_trade"],
            markets,
        )

        trades = trade_rows(conn)
        write_csv(
            OUT_DIR / "trades_enriched.csv",
            [
                "trade_id",
                "entry_ms",
                "entry_utc",
                "market_id",
                "question",
                "strategy",
                "side",
                "entry_price",
                "size",
                "status",
                "fill_status",
                "execution_fidelity",
                "resolved_ms",
                "resolved_utc",
                "settlement_price",
                "fee_amount",
                "pnl_net",
            ],
            [
                (
                    trade_id,
                    entry_ms,
                    dt(entry_ms),
                    market_id,
                    question,
                    strategy,
                    side,
                    entry_price,
                    size,
                    status,
                    fill_status,
                    fidelity,
                    resolved_ms,
                    dt(resolved_ms),
                    settlement_price,
                    fee_amount,
                    pnl_net,
                )
                for (
                    trade_id,
                    entry_ms,
                    market_id,
                    question,
                    strategy,
                    side,
                    entry_price,
                    size,
                    status,
                    fill_status,
                    fidelity,
                    resolved_ms,
                    settlement_price,
                    fee_amount,
                    pnl_net,
                ) in trades
            ],
        )

        write_csv(
            OUT_DIR / "signal_status.csv",
            [
                "strategy",
                "decision_status",
                "rejection_reason",
                "count",
                "avg_quote_age_ms",
                "avg_book_staleness_ms",
                "avg_arrival_delay_ms",
                "avg_expected_edge",
            ],
            signal_status(conn),
        )

        write_csv(
            OUT_DIR / "rejection_rollups.csv",
            ["strategy", "reason", "total_count", "rollup_rows", "first_ms", "last_ms"],
            rejection_rollups(conn),
        )

        write_csv(
            OUT_DIR / "feed_events_by_type.csv",
            ["source", "event_type", "count", "first_ms", "last_ms"],
            feed_event_counts(conn),
        )

        write_csv(
            OUT_DIR / "feed_health_events.csv",
            ["source", "event_type", "count", "first_ms", "last_ms"],
            feed_health(conn),
        )

        log_counts = log_counters()
        live_summary = live_snapshot_summary(conn)[0]
        top_losses = sorted(trades, key=lambda r: r[-1])[:10]
        last_30 = trades[-30:]
        worst_markets = markets[:10]
        strategy_rows = strategy_attribution(conn)
        rejection_rows = rejection_rollups(conn)[:12]
        feed_health_rows = feed_health(conn)
        feed_event_rows = feed_event_counts(conn)[:12]

        feed_md = [
            "# Run 012 Feed and Runtime Health",
            "",
            f"Feed events covered `{summary['first_tick_utc']}` to `{summary['last_tick_utc']}` with `{summary['feed_events']:,}` raw rows.",
            "",
            "## Feed Event Mix",
            "",
        ]
        for source, event_type, count, first_ms, last_ms in feed_event_rows:
            feed_md.append(f"- `{source}` / `{event_type}`: `{count:,}` rows from `{dt(first_ms)}` to `{dt(last_ms)}`")
        feed_md.extend(["", "## Feed Health Events", ""])
        for source, event_type, count, first_ms, last_ms in feed_health_rows:
            feed_md.append(f"- `{source}` / `{event_type}`: `{count:,}` events from `{dt(first_ms)}` to `{dt(last_ms)}`")
        feed_md.extend(["", "## Log Counters", ""])
        for name, counts in log_counts.items():
            feed_md.append(
                f"- `{name}`: `{counts['lines']:,}` lines, `{counts['warn']:,}` warn-like hits, `{counts['error']:,}` error-like hits, "
                f"`{counts['feed_disconnect']:,}` feed disconnects, `{counts['strategy_rejection_rollup']:,}` rejection rollups, "
                f"`{counts['readonly_rollup']:,}` readonly rollups, `{counts['account_refresh_failed']:,}` account refresh failures"
            )
        feed_md.extend(
            [
                "",
                "## Read",
                "",
                "The no-trade period after the final trade is not explained by missing collectors. Feed and signal rows continue after the halt. The health work item remains to reduce CLOB churn and sidecar reconnect noise because those events add operational noise and can contribute to stale-feature rejections.",
                "",
            ]
        )
        (OUT_DIR / "feed_health.md").write_text("\n".join(feed_md))

        events = [
            "# Run 012 Major Events",
            "",
            f"- First feed tick: `{summary['first_tick_utc']}`",
            f"- First signal: `{summary['first_signal_utc']}`",
            f"- First trade entry: `{summary['first_trade_utc']}`",
            f"- High water mark: `{usd(summary['high_water_mark'])}` at `{summary['high_water_mark_utc']}`",
            f"- Drawdown breach: `{summary['drawdown_breach']['timestamp_utc']}` at `{pct(summary['drawdown_breach']['drawdown_pct'])}` drawdown",
            f"- Last trade entry: `{summary['last_trade_utc']}`",
            f"- Last signal: `{summary['last_signal_utc']}`",
            f"- Last feed tick: `{summary['last_tick_utc']}`",
            "",
            "## Top Losing Trades",
            "",
        ]
        for row in top_losses:
            events.append(
                f"- trade `{row[0]}` `{dt(row[1])}` `{row[4]}` `{row[5]}` market `{row[2]}` size `{row[7]:.2f}` entry `{row[6]:.4f}` pnl `{usd(row[-1])}`"
            )
        events.extend(["", "## Last 30 Trades Before Halt", ""])
        for row in last_30:
            events.append(
                f"- trade `{row[0]}` `{dt(row[1])}` `{row[4]}` `{row[5]}` market `{row[2]}` pnl `{usd(row[-1])}`"
            )
        events.extend(["", "## Worst Markets", ""])
        for market_id, question, first_ms, last_ms, count, pnl_net, worst_trade, best_trade in worst_markets:
            events.append(
                f"- market `{market_id}` `{dt(first_ms)}` trades `{count}` pnl `{usd(pnl_net)}` worst `{usd(worst_trade)}` best `{usd(best_trade)}` question `{question}`"
            )
        (OUT_DIR / "major_events.md").write_text("\n".join(events))

        params = """BACKTEST_SETTLEMENT_MODE=observed_market_resolution
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false
LATENCY_ARB_ENABLED=true
LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008
LATENCY_ARB_MIN_ASK=0.30
LATENCY_ARB_MAX_ASK=0.60
LATENCY_ARB_COOLDOWN_MS=60000
LATENCY_ARB_ADAPTIVE_WINDOW_MS=1800000
LATENCY_ARB_MAX_POSITION_FRACTION=0.125
SPREAD_CAPTURE_ENABLED=true
SPREAD_CAPTURE_THRESHOLD=0.970
SPREAD_CAPTURE_MAX_LEG_SKEW_MS=25
SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S=8
SPREAD_CAPTURE_MAX_POSITION_FRACTION=0.05
CALM_PERSISTENCE_ENABLED=true
CALM_PERSISTENCE_MIN_WINDOW_TIME_MS=30000
CALM_PERSISTENCE_MAX_WINDOW_TIME_MS=90000
CALM_PERSISTENCE_MAX_ASK=0.65
CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS=6
CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD=1.0
CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS=80
CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S=1
CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S=100
CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION=0.5
CALM_PERSISTENCE_MAX_FAIR_BIAS=0.35
CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.05
CALM_PERSISTENCE_MAX_POSITION_FRACTION=0.05
MAX_POSITION_FRACTION=0.05
MAX_DRAWDOWN_PCT=0.50
MIN_WINDOW_TIME_MS=90000
TAKER_FEE_RATE=0.072
TAKER_FEE_EXPONENT=1
SIM_ORDER_LATENCY_MS=250
"""
        (OUT_DIR / "baseline_params.env").write_text(params)

        postmortem = [
            "# Run 012 Forensic Postmortem",
            "",
            "This postmortem is descriptive only, not sweep-grade. The archive is useful for realized PnL, drawdown, halt behavior, strategy attribution, and operational health. It must not be used for trusted parameter optimization because compact capture omitted Binance `bookTicker` rows that live decisions used in memory. This local run was originally discussed as server run 013 before local renumbering.",
            "",
            "## Executive Read",
            "",
            f"Run 012 grew from `{usd(summary['start_balance'])}` to a final archived balance of `{usd(summary['final_balance'])}`. The run was strongly positive in absolute PnL, but it hit the configured hard drawdown boundary after a large peak-to-trough giveback from `{usd(summary['high_water_mark'])}` at `{summary['high_water_mark_utc']}` to `{usd(summary['final_balance'])}` at `{summary['drawdown_breach']['timestamp_utc']}`.",
            "",
            f"The breach was `{pct(summary['drawdown_breach']['drawdown_pct'])}` against `MAX_DRAWDOWN_PCT=50%`. The process continued collecting feed and signal data after the halt, but execution was effectively stopped. Current code reports those later capital blocks through `strategy_sleeve_exhausted`, which is operationally misleading and should become an explicit `max_drawdown_exceeded` state.",
            "",
            "## Strategy Contribution",
            "",
        ]
        for strategy, trades_count, wins, losses, pnl_net, avg_pnl_net, worst_trade, best_trade, fees, avg_size, total_size in strategy_rows:
            postmortem.append(
                f"- `{strategy}`: `{trades_count}` trades, `{wins}` wins, `{losses}` losses, pnl `{usd(pnl_net)}`, avg `{usd(avg_pnl_net)}`, worst `{usd(worst_trade)}`, best `{usd(best_trade)}`, fees `{usd(fees)}`"
            )
        postmortem.extend(
            [
                "",
                "## Why Positive Latency-Arb Did Not Save The Run",
                "",
                "Latency-arb was positive overall, but the run-level risk was path-dependent. The account reached a high-water mark above the final balance, then a cluster of losses reduced equity by roughly half from that peak. A strategy can have positive total contribution and still leave the portfolio halted if sizing, timing, and correlated late losses drive the peak-to-trough path through the hard stop.",
                "",
                "## Rejection and Signal Quality",
                "",
            ]
        )
        for strategy, reason, total_count, rollup_rows, first_ms, last_ms in rejection_rows:
            postmortem.append(
                f"- `{strategy}` / `{reason}`: `{total_count:,}` rejects across `{rollup_rows:,}` rollups from `{dt(first_ms)}` to `{dt(last_ms)}`"
            )
        postmortem.extend(
            [
                "",
                "## Live Readonly Account Snapshot",
                "",
                f"- snapshots: `{live_summary[0]}` from `{dt(live_summary[1])}` to `{dt(live_summary[2])}`",
                f"- cash available range: `{usd(live_summary[4])}` to `{usd(live_summary[5])}`",
                f"- total equity range: `{usd(live_summary[7])}` to `{usd(live_summary[8])}`",
                "",
                "## Baseline Replay Status",
                "",
            ]
        )
        if baseline:
            deltas = summary["baseline_deltas"]
            postmortem.extend(
                [
                    f"- exact replay final balance: `{usd(baseline['final_balance'])}`",
                    f"- final balance delta versus archive: `{usd(deltas['final_balance'])}`",
                    f"- trade count delta: `{deltas['trade_count']}`",
                    f"- max drawdown delta: `{deltas['max_drawdown_pp']:.2f}pp`",
                ]
            )
        else:
            postmortem.append("- exact replay has not been run yet.")
        postmortem.extend(
            [
                "",
                "## Next Decisions",
                "",
                "- Do not use raw PnL alone for parameter selection. Rank candidates by PnL, max drawdown, drawdown phase behavior, trade count, and strategy concentration.",
                "- Fix `max_drawdown_exceeded` labeling and dashboard hard-stop UX after analysis, not before the baseline comparison.",
                "- Treat CLOB churn and sidecar reconnect noise as hardening work. They were not the primary no-trade cause, but they are relevant to trust and operator noise.",
                "",
            ]
        )
        (OUT_DIR / "postmortem.md").write_text("\n".join(postmortem))
    finally:
        conn.close()


if __name__ == "__main__":
    write_outputs()
