# Strategy And Risk

This chapter explains what the bot is trying to exploit, what is enabled today, and what risk gates protect the system before any future funded trading.

## Market Scope

The `paint` bot targets Polymarket BTC 5-minute Up/Down markets. Each window has UP and DOWN outcome tokens, a start time, an end time, CLOB market metadata, tick-size and min-size constraints, and a settlement outcome after resolution.

Research and readonly deployments use the same strategy engine. That is intentional: paper, readonly, backtest, and future live paths should not drift into separate trading systems.

## Strategy Families

`latency-arb` is the current operating strategy. It looks for a Binance-led move before Polymarket reprices. A candidate is valid only when feed freshness, CLOB top-of-book state, ask bounds, fees, min size, bankroll, and duplicate/exposure checks all pass.

`spread-capture` looks for UP plus DOWN asks that are cheap enough after fees. This strategy is structurally non-atomic because the two legs are independent orders. One filled leg without the other is residual exposure, not a harmless detail. The code supports the family, but it is disabled in current remote operation.

`calm-persistence` looks for quiet late-window persistence where the currently winning side remains underpriced. It is research-capable but disabled in current remote operation.

## Current Remote Profile

Current Docker `live_readonly` deployment uses a Run-012-style latency-only profile:

```bash
EXECUTION_MODE=live_readonly
FEED_EVENT_STORAGE_PROFILE=replay_grade
STARTING_BALANCE=100
LATENCY_ARB_ENABLED=true
LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008
LATENCY_ARB_MIN_ASK=0.30
LATENCY_ARB_MAX_ASK=0.60
LATENCY_ARB_COOLDOWN_MS=60000
LATENCY_ARB_ADAPTIVE_WINDOW_MS=1800000
LATENCY_ARB_MAX_POSITION_FRACTION=0.125
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
MAX_POSITION_FRACTION=0.05
MAX_DRAWDOWN_PCT=0.50
MIN_WINDOW_TIME_MS=90000
TAKER_FEE_RATE=0.07
TAKER_FEE_EXPONENT=1
SIM_ORDER_LATENCY_MS=250
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false
```

These values are declared in `docker-compose.live-readonly.yml` and `bots/paint/src/config.rs`. The live deployed truth is the dashboard Parameters page or `run_metadata.runtime_config_snapshot`.

## Candidate Lifecycle

A strategy candidate is only the beginning of a decision. Before it becomes a paper trade or live intent, the runtime checks:

* market and window activation
* Binance, Chainlink, and CLOB freshness
* CLOB liquidity and quote age
* ask bounds and expected edge
* fee model and slippage estimate
* venue tick size and min size where venue metadata exists
* bankroll and per-strategy sleeve limits
* duplicate exposure, open-position pressure, and pending settlement reserve
* circuit breaker and drawdown state

Rejected candidates are still useful. The bot stores signal metrics and aggregated rejection summaries so analysts can distinguish "strategy saw nothing" from "strategy saw candidates but risk, marketability, or data-quality gates rejected them."

## Bankroll And Exposure

Paper and live runtime decisions use in-memory bankroll and exposure state. The hot path does not query SQLite to decide whether a position already exists. State is seeded at startup and then updated from accepted intents, paper fills, live fills, misses, settlement, reconciliation feedback, and reservation releases.

Important controls:

* `MAX_POSITION_FRACTION`
* per-family max-position fractions
* `MAX_DRAWDOWN_PCT`
* pending-settlement reserve knobs
* live cash cap, single-order cap, open-notional cap, daily loss cap, and session drawdown cap

Pending settlement is not spendable cash. For future funded sessions, only observed available collateral may be spent.

## Live Risk Posture

Real-money trading is not currently deployed or armed. If it resumes later, the first canary posture remains:

* latency-arb only
* small bankroll around `$100`
* calm and spread disabled by runtime config
* FOK/FAK only
* strict cash, order, open-notional, daily-loss, and session-drawdown caps
* geoblock, account, user-stream, replay capture, and reconciliation gates
* terminal halt on unknown order state, critical reconciliation, or persistent venue/account/user-stream degradation

Spread-capture needs explicit residual-exposure handling before funded enablement. Calm-persistence needs fresh research evidence before funded enablement.

## HFT Safety Rule

Low latency here means the decision path is bounded and predictable, not that the system ignores evidence. The hot path updates in-memory feed state, computes features, evaluates the latest decision snapshot, and enqueues persistence or submission work. It must not run replay validators, DB integrity checks, whole-table scans, dashboard summaries, sidecar calls, or account/control polling.

Replay-grade capture remains mandatory. The fix for latency is worker isolation and compact storage, not dropping decision inputs.

## Backtesting Relationship

Backtests replay typed feed inputs through the same feature and strategy logic. A sweep is trusted only when:

* public inputs validate as `sweep_grade`
* the current backtester reports `backtest_ready`
* large intervals use `prepared_backtest` for practical sweep performance
* funded live intervals additionally validate as `research_grade_live`

If any of those gates fail, the data may still support operational review, but it is not a basis for parameter selection.
