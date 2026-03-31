/**
 * BacktestRunner — the heart of the backtesting engine.
 * Replays historical ticks through real strategy code.
 */

import BetterSqlite3 from "better-sqlite3";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { setClock, resetClock } from "../clock.js";
import { CONFIG } from "../config.js";
import { Database } from "../db/database.js";
import { LatencyArbStrategy } from "../strategies/latency-arb.js";
import { SpreadCaptureStrategy } from "../strategies/spread-capture.js";
import { BankrollManager } from "../bankroll-manager.js";
import { PositionManager } from "../position-manager.js";
import { CircuitBreaker } from "../circuit-breaker.js";
import { TrendTracker } from "../trend-tracker.js";
import { TickReplay } from "./tick-replay.js";
import { FeedState } from "./feed-state.js";
import { MomentumCalculator } from "./momentum.js";
import { WindowManager } from "./window-manager.js";
import type { StrategyContext, Strategy, Signal, SignalDirection, MarketWindow } from "../types.js";
import type { MarketSettlement } from "./window-manager.js";

export interface BacktestResult {
  startTime: number;
  endTime: number;
  durationHours: number;
  elapsedSeconds: number;
  totalTicks: number;
  totalWindows: number;
  signals: number;
  trades: number;
  wins: number;
  losses: number;
  winRate: number;
  finalBalance: number;
  totalPnl: number;
  maxDrawdownPct: number;
  highWaterMark: number;
}

export interface BacktestOptions {
  dataDbPath: string;
  resultsDbPath: string;
  startTime: number;
  endTime: number;
  startingBalance: number;
  quiet?: boolean;
  /** Pre-loaded raw ticks (avoids re-reading from DB on each sweep run). */
  cachedTicks?: unknown[];
}

export function runBacktest(options: BacktestOptions): BacktestResult {
  const { dataDbPath, resultsDbPath, startTime, endTime, startingBalance, quiet, cachedTicks } = options;
  const t0 = Date.now();

  // Open data DB (read-only) — skip if ticks are pre-loaded
  const dataDb = cachedTicks ? null : new BetterSqlite3(dataDbPath, { readonly: true });

  // Create results DB (uses same schema as live bot)
  mkdirSync(dirname(resultsDbPath), { recursive: true });
  const resultsDb = new Database(resultsDbPath);

  // Initialize components (real code, unchanged)
  const bankroll = new BankrollManager(startingBalance, resultsDb);
  const positionManager = new PositionManager(resultsDb, bankroll);
  const circuitBreaker = new CircuitBreaker();
  const trendTracker = new TrendTracker();

  const strategies: Strategy[] = [
    new LatencyArbStrategy(),
    new SpreadCaptureStrategy(),
  ];

  // Initialize backtesting infrastructure
  const tickReplay = cachedTicks
    ? new TickReplay(cachedTicks as any[])
    : new TickReplay(dataDb!, startTime, endTime);
  const feedState = new FeedState();
  const momentum = new MomentumCalculator(CONFIG.MOMENTUM_WINDOW_MS);

  // WindowManager needs DB access for market data; use dataDb or open one
  const marketDb = dataDb ?? new BetterSqlite3(dataDbPath, { readonly: true });
  const windowManager = new WindowManager(marketDb, startTime, endTime);

  if (!quiet) {
    console.log(
      `Backtesting ${((endTime - startTime) / 3_600_000).toFixed(1)}h | ` +
      `${tickReplay.totalTicks.toLocaleString()} ticks | ` +
      `${windowManager.totalWindows} windows | ` +
      `balance=$${startingBalance}`,
    );
  }

  let signalCount = 0;
  let replayTs = 0;

  const origResolve = positionManager.resolveWindow.bind(positionManager);
  positionManager.resolveWindow = (window: MarketWindow, openPrice: number, closePrice: number) => {
    const outcome: SignalDirection = closePrice >= openPrice ? "UP" : "DOWN";
    const trades = resultsDb.getOpenTradesForMarket(window.marketId);
    origResolve(window, openPrice, closePrice);
    for (const trade of trades) {
      const won = trade.side === outcome;
      trendTracker.recordOutcome(trade.side, won);
      circuitBreaker.recordResult(won);
    }
  };

  let group = tickReplay.next();
  while (group !== null) {
    replayTs = group.timestamp;
    setClock(() => replayTs);

    feedState.update(group);
    if (group.binance?.price != null) {
      momentum.push(group.binance.price, group.timestamp);
    }

    const events = windowManager.advance(group.timestamp);

    if (events.closed) {
      const closed = events.closed;
      const mw = windowManager.toMarketWindow(closed);
      resultsDb.upsertMarket(mw);
      positionManager.resolveWindow(mw, closed.openPrice, closed.closePrice);
      resultsDb.resolveMarket(mw.marketId, "resolved");
    }

    if (events.opened) {
      const opened = events.opened;
      const mw = windowManager.toMarketWindow(opened);
      resultsDb.upsertMarket(mw);
      feedState.bookState = { up: null, down: null };
    }

    if (!windowManager.current || feedState.binancePrice === null) {
      group = tickReplay.next();
      continue;
    }

    const ctx: StrategyContext = {
      binancePrice: feedState.binancePrice,
      binanceMomentum: momentum.get(),
      chainlinkPrice: feedState.chainlinkPrice,
      bookState: feedState.bookState,
      windowTimeRemainingMs: windowManager.current.endTime - group.timestamp,
    };

    // Circuit breaker
    if (!circuitBreaker.canTrade()) {
      group = tickReplay.next();
      continue;
    }

    // Evaluate strategies (same flow as main.ts:125-172)
    for (const strategy of strategies) {
      const result = strategy.evaluate(ctx);
      if (result === null) continue;

      const signals = Array.isArray(result) ? result : [result];
      const isBatch = Array.isArray(result) && result.length > 1;

      if (isBatch) {
        for (const signal of signals) {
          resultsDb.logSignal(signal);
          signalCount++;
        }
        const mw = windowManager.toMarketWindow(windowManager.current);
        positionManager.tryOpenSpread(signals, mw);
      } else {
        for (const signal of signals) {
          if (trendTracker.shouldSuppress(signal.direction)) {
            resultsDb.logSignal(signal);
            signalCount++;
            continue;
          }
          resultsDb.logSignal(signal);
          signalCount++;
          const mw = windowManager.toMarketWindow(windowManager.current);
          positionManager.tryOpen(signal, mw);
        }
      }
    }

    group = tickReplay.next();
  }

  // Reset clock
  resetClock();

  // Gather results
  const stats = bankroll.getStats();
  const elapsed = (Date.now() - t0) / 1000;

  const result: BacktestResult = {
    startTime,
    endTime,
    durationHours: (endTime - startTime) / 3_600_000,
    elapsedSeconds: elapsed,
    totalTicks: tickReplay.totalTicks,
    totalWindows: windowManager.totalWindows,
    signals: signalCount,
    trades: stats.totalTrades,
    wins: stats.wins,
    losses: stats.losses,
    winRate: stats.winRate,
    finalBalance: stats.currentBalance,
    totalPnl: stats.totalPnl,
    maxDrawdownPct: stats.maxDrawdownPct,
    highWaterMark: stats.highWaterMark,
  };

  if (!quiet) {
    console.log(
      `PnL=$${result.totalPnl.toFixed(2)} | ` +
      `WR=${(result.winRate * 100).toFixed(1)}% | ` +
      `Trades=${result.trades} (${result.wins}W/${result.losses}L) | ` +
      `MaxDD=${(result.maxDrawdownPct * 100).toFixed(1)}% | ` +
      `Peak=$${result.highWaterMark.toFixed(2)} | ` +
      `${result.durationHours.toFixed(1)}h replayed in ${result.elapsedSeconds.toFixed(1)}s`,
    );
  }

  // Cleanup
  resultsDb.close();
  if (dataDb) dataDb.close();
  if (!dataDb && marketDb) marketDb.close();

  return result;
}
