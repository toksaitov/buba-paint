import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { Database } from "./db/database.js";

const log = createLogger("bankroll");

interface StrategyRecord {
  wins: number;
  losses: number;
}

interface TradeResult {
  strategy: string;
  won: boolean;
}

export interface BankrollStats {
  startingBalance: number;
  currentBalance: number;
  highWaterMark: number;
  maxDrawdownPct: number;
  totalTrades: number;
  wins: number;
  losses: number;
  winRate: number;
  totalPnl: number;
}

export class BankrollManager {
  private startingBalance: number;
  private currentBalance: number;
  private highWaterMark: number;
  private peakDrawdownPct = 0;
  private totalWins = 0;
  private totalLosses = 0;
  private totalTrades = 0;
  private reservedCapital = 0;
  private peakDdPauseUntil = 0;

  // Per-strategy lifetime stats
  private strategyStats = new Map<string, StrategyRecord>();

  // Rolling window of recent trade results for adaptive Kelly
  private recentResults: TradeResult[] = [];

  constructor(startingBalance: number, private db: Database) {
    // Try to recover balance from DB; fall back to config value
    const recovered = db.getLatestBalance();
    if (recovered !== null) {
      this.startingBalance = startingBalance;
      this.currentBalance = recovered;
      this.highWaterMark = Math.max(startingBalance, recovered);
      log.info(`Recovered balance from DB: $${recovered.toFixed(2)}`);
    } else {
      this.startingBalance = startingBalance;
      this.currentBalance = startingBalance;
      this.highWaterMark = startingBalance;
      db.logBalanceEvent("init", null, 0, startingBalance);
      log.info(`Initialized with $${startingBalance.toFixed(2)}`);
    }
  }

  /**
   * Reserve capital for a single-leg trade.
   * Returns token count to buy (0 = can't afford / shouldn't trade).
   */
  reserveCapital(entryPrice: number, confidence: number = 1.0, strategy: string = "unknown"): number {
    if (!this.canTrade()) return 0;
    if (entryPrice <= 0 || entryPrice >= 1.0) return 0;

    const available = this.currentBalance - this.reservedCapital;
    if (available <= 0) return 0;

    const fraction = this.getPositionFraction(entryPrice, confidence, strategy);
    if (fraction <= 0) return 0;

    const kellyNotional = this.currentBalance * fraction;
    const maxPositionUsd = this.currentBalance * CONFIG.MAX_POSITION_USD_FRACTION;
    const notional = Math.min(kellyNotional, available, maxPositionUsd);
    let tokenCount = Math.floor(notional / entryPrice);

    // Minimum bet floor: bump dust bets up to MIN_BET_USD if balance allows
    if (tokenCount > 0 && tokenCount * entryPrice < CONFIG.MIN_BET_USD) {
      const minTokens = Math.floor(CONFIG.MIN_BET_USD / entryPrice);
      if (minTokens * entryPrice <= available && minTokens * entryPrice <= maxPositionUsd) {
        tokenCount = minTokens;
      }
    }

    if (tokenCount <= 0) return 0;

    const cost = tokenCount * entryPrice;
    this.reservedCapital += cost;

    log.info(
      `Reserved $${cost.toFixed(2)} (${tokenCount} tokens @ $${entryPrice.toFixed(3)}) | ` +
      `fraction=${(fraction * 100).toFixed(1)}% confidence=${confidence.toFixed(2)} strategy=${strategy} | ` +
      `balance=$${this.currentBalance.toFixed(2)} reserved=$${this.reservedCapital.toFixed(2)}`,
    );

    return tokenCount;
  }

  /**
   * Reserve capital for a spread-capture pair (both legs).
   * Returns equal token counts for both legs, sized on total pair cost.
   */
  reserveSpreadCapital(
    upAsk: number,
    downAsk: number,
    confidence: number = 1.0,
  ): { upTokens: number; downTokens: number } {
    if (!this.canTrade()) return { upTokens: 0, downTokens: 0 };
    if (upAsk <= 0 || downAsk <= 0 || upAsk >= 1.0 || downAsk >= 1.0) {
      return { upTokens: 0, downTokens: 0 };
    }

    const available = this.currentBalance - this.reservedCapital;
    if (available <= 0) return { upTokens: 0, downTokens: 0 };

    const totalAskPerUnit = upAsk + downAsk;
    const maxPositionUsd = this.currentBalance * CONFIG.MAX_POSITION_USD_FRACTION;
    const maxFromBalance = this.currentBalance * CONFIG.MAX_POSITION_FRACTION;

    // For spread capture, use fixed fraction (not Kelly — it's a hedge, not a directional bet)
    const notional = Math.min(maxFromBalance, available, maxPositionUsd);
    const pairUnits = Math.floor(notional / totalAskPerUnit);

    if (pairUnits <= 0) return { upTokens: 0, downTokens: 0 };

    const totalCost = pairUnits * totalAskPerUnit;
    this.reservedCapital += totalCost;

    log.info(
      `Reserved spread: ${pairUnits} pairs ($${totalCost.toFixed(2)}) | ` +
      `UP ${pairUnits}@${upAsk.toFixed(3)} + DOWN ${pairUnits}@${downAsk.toFixed(3)} | ` +
      `balance=$${this.currentBalance.toFixed(2)} reserved=$${this.reservedCapital.toFixed(2)}`,
    );

    return { upTokens: pairUnits, downTokens: pairUnits };
  }

  /**
   * Apply trade result after settlement. Updates balance, stats, and DB.
   */
  applyTradeResult(
    tradeId: number,
    entryPrice: number,
    size: number,
    settlementPrice: number,
    strategy: string = "unknown",
  ): void {
    const cost = entryPrice * size;
    const payout = settlementPrice * size;
    const pnl = payout - cost;

    this.reservedCapital = Math.max(0, this.reservedCapital - cost);
    this.currentBalance += pnl;
    this.totalTrades++;

    const won = pnl > 0;
    if (won) {
      this.totalWins++;
    } else {
      this.totalLosses++;
    }

    // Per-strategy tracking
    const stats = this.strategyStats.get(strategy) ?? { wins: 0, losses: 0 };
    if (won) stats.wins++;
    else stats.losses++;
    this.strategyStats.set(strategy, stats);

    // Rolling window
    this.recentResults.push({ strategy, won });
    if (this.recentResults.length > CONFIG.KELLY_ROLLING_WINDOW) {
      this.recentResults.shift();
    }

    if (this.currentBalance > this.highWaterMark) {
      this.highWaterMark = this.currentBalance;
    }

    const drawdown = this.getDrawdownPct();
    if (drawdown > this.peakDrawdownPct) {
      this.peakDrawdownPct = drawdown;
    }

    this.db.logBalanceEvent("trade_close", tradeId, pnl, this.currentBalance);

    const stratWR = this.getStrategyWinRate(strategy);
    log.info(
      `Trade #${tradeId} settled: ${won ? "WIN" : "LOSS"} $${pnl.toFixed(2)} | ` +
      `balance=$${this.currentBalance.toFixed(2)} | ` +
      `W/L=${this.totalWins}/${this.totalLosses} (${(this.getWinRate() * 100).toFixed(0)}%) | ` +
      `${strategy} WR=${(stratWR * 100).toFixed(0)}% | ` +
      `drawdown=${(drawdown * 100).toFixed(1)}%`,
    );
  }

  /**
   * Get win rate for a specific strategy from the rolling window.
   * Falls back to lifetime stats if not enough rolling data.
   */
  getStrategyWinRate(strategy: string): number {
    // Try rolling window first
    const rollingForStrategy = this.recentResults.filter((r) => r.strategy === strategy);
    if (rollingForStrategy.length >= 5) {
      const wins = rollingForStrategy.filter((r) => r.won).length;
      return wins / rollingForStrategy.length;
    }

    // Fall back to lifetime per-strategy stats
    const stats = this.strategyStats.get(strategy);
    if (!stats) return 0;
    const total = stats.wins + stats.losses;
    return total > 0 ? stats.wins / total : 0;
  }

  /**
   * Calculate position fraction based on Kelly criterion or fixed fraction.
   * Uses per-strategy win rate and steeper confidence curve.
   */
  private getPositionFraction(entryPrice: number, confidence: number, strategy: string): number {
    let fraction: number;

    // Use per-strategy trade count for Kelly activation, not global
    const stratStats = this.strategyStats.get(strategy);
    const stratTotal = stratStats ? stratStats.wins + stratStats.losses : 0;

    if (stratTotal >= CONFIG.MIN_TRADES_FOR_KELLY) {
      const winRate = this.getStrategyWinRate(strategy);
      fraction = this.getKellyFraction(entryPrice, winRate);
    } else {
      fraction = CONFIG.MAX_POSITION_FRACTION;
    }

    // Steeper confidence curve: confidence 0.50 → 0x, 0.60 → 0.25x, 0.90 → 1.0x
    const confidenceMultiplier = Math.max(0, (confidence - 0.5) * 2.5);
    fraction *= confidenceMultiplier;

    // Cap at max fraction
    return Math.min(fraction, CONFIG.MAX_POSITION_FRACTION);
  }

  /**
   * Kelly Criterion for binary outcome markets.
   * f* = (b*p - q) / b, where b = (1-X)/X, p = win rate, q = 1-p
   */
  private getKellyFraction(entryPrice: number, winRate: number): number {
    if (winRate < CONFIG.MIN_WIN_RATE_FOR_KELLY) return CONFIG.MIN_KELLY_FLOOR;

    const b = (1 - entryPrice) / entryPrice;
    const p = winRate;
    const q = 1 - p;
    const fullKelly = (b * p - q) / b;

    if (fullKelly <= 0) return CONFIG.MIN_KELLY_FLOOR;

    return fullKelly * CONFIG.KELLY_FRACTION;
  }

  canTrade(): boolean {
    if (this.currentBalance < CONFIG.MIN_BALANCE_THRESHOLD) {
      log.warn(`Balance $${this.currentBalance.toFixed(2)} below minimum $${CONFIG.MIN_BALANCE_THRESHOLD}`);
      return false;
    }
    if (this.getDrawdownPct() >= CONFIG.MAX_DRAWDOWN_PCT) {
      log.warn(`Drawdown ${(this.getDrawdownPct() * 100).toFixed(1)}% exceeds max ${(CONFIG.MAX_DRAWDOWN_PCT * 100).toFixed(0)}%`);
      return false;
    }

    // Trailing peak drawdown pause: if balance dropped ≥30% from all-time high,
    // pause trading for 1 hour to let volatility subside and prevent cascading losses.
    const peakDd = this.getDrawdownPct();
    const now = Date.now();
    if (peakDd >= CONFIG.PEAK_DD_PAUSE_PCT) {
      if (this.peakDdPauseUntil === 0) {
        // First trigger — start the pause timer
        this.peakDdPauseUntil = now + CONFIG.PEAK_DD_PAUSE_MS;
        log.warn(
          `Peak drawdown pause triggered: ${(peakDd * 100).toFixed(1)}% from peak $${this.highWaterMark.toFixed(2)}. ` +
          `Pausing for ${Math.round(CONFIG.PEAK_DD_PAUSE_MS / 60_000)} minutes.`,
        );
      }
      if (now < this.peakDdPauseUntil) {
        return false;
      }
      // Pause timer expired — allow trading, reset for next trigger
      this.peakDdPauseUntil = 0;
    } else {
      // Drawdown recovered below threshold — clear any pending pause
      this.peakDdPauseUntil = 0;
    }

    return true;
  }

  getBalance(): number {
    return this.currentBalance;
  }

  getWinRate(): number {
    return this.totalTrades > 0 ? this.totalWins / this.totalTrades : 0;
  }

  getDrawdownPct(): number {
    if (this.highWaterMark <= 0) return 0;
    return (this.highWaterMark - this.currentBalance) / this.highWaterMark;
  }

  getStats(): BankrollStats {
    return {
      startingBalance: this.startingBalance,
      currentBalance: this.currentBalance,
      highWaterMark: this.highWaterMark,
      maxDrawdownPct: this.peakDrawdownPct,
      totalTrades: this.totalTrades,
      wins: this.totalWins,
      losses: this.totalLosses,
      winRate: this.getWinRate(),
      totalPnl: this.currentBalance - this.startingBalance,
    };
  }
}
