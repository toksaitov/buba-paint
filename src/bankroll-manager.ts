import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { Database } from "./db/database.js";

const log = createLogger("bankroll");

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
   * Reserve capital for a trade. Returns token count to buy (0 = can't afford / shouldn't trade).
   */
  reserveCapital(entryPrice: number, confidence: number = 1.0): number {
    if (!this.canTrade()) return 0;
    if (entryPrice <= 0 || entryPrice >= 1.0) return 0;

    const available = this.currentBalance - this.reservedCapital;
    if (available <= 0) return 0;

    const fraction = this.getPositionFraction(entryPrice, confidence);
    if (fraction <= 0) return 0;

    const maxNotional = this.currentBalance * fraction;
    const notional = Math.min(maxNotional, available);
    const tokenCount = Math.floor(notional / entryPrice);

    if (tokenCount <= 0) return 0;

    const cost = tokenCount * entryPrice;
    this.reservedCapital += cost;

    log.info(
      `Reserved $${cost.toFixed(2)} (${tokenCount} tokens @ $${entryPrice.toFixed(3)}) | ` +
      `fraction=${(fraction * 100).toFixed(1)}% confidence=${confidence.toFixed(2)} | ` +
      `balance=$${this.currentBalance.toFixed(2)} reserved=$${this.reservedCapital.toFixed(2)}`,
    );

    return tokenCount;
  }

  /**
   * Apply trade result after settlement. Updates balance, stats, and DB.
   */
  applyTradeResult(tradeId: number, entryPrice: number, size: number, settlementPrice: number): void {
    const cost = entryPrice * size;
    const payout = settlementPrice * size;
    const pnl = payout - cost;

    this.reservedCapital = Math.max(0, this.reservedCapital - cost);
    this.currentBalance += pnl;
    this.totalTrades++;

    if (pnl > 0) {
      this.totalWins++;
    } else {
      this.totalLosses++;
    }

    if (this.currentBalance > this.highWaterMark) {
      this.highWaterMark = this.currentBalance;
    }

    const drawdown = this.getDrawdownPct();
    if (drawdown > this.peakDrawdownPct) {
      this.peakDrawdownPct = drawdown;
    }

    this.db.logBalanceEvent("trade_close", tradeId, pnl, this.currentBalance);

    log.info(
      `Trade #${tradeId} settled: ${pnl >= 0 ? "WIN" : "LOSS"} $${pnl.toFixed(2)} | ` +
      `balance=$${this.currentBalance.toFixed(2)} | ` +
      `W/L=${this.totalWins}/${this.totalLosses} (${(this.getWinRate() * 100).toFixed(0)}%) | ` +
      `drawdown=${(drawdown * 100).toFixed(1)}%`,
    );
  }

  /**
   * Calculate position fraction based on Kelly criterion or fixed fraction.
   */
  private getPositionFraction(entryPrice: number, confidence: number): number {
    let fraction: number;

    if (this.totalTrades >= CONFIG.MIN_TRADES_FOR_KELLY) {
      fraction = this.getKellyFraction(entryPrice, this.getWinRate());
    } else {
      fraction = CONFIG.MAX_POSITION_FRACTION;
    }

    // Scale by signal confidence
    fraction *= confidence;

    // Cap at max fraction
    return Math.min(fraction, CONFIG.MAX_POSITION_FRACTION);
  }

  /**
   * Kelly Criterion for binary outcome markets.
   * f* = (b*p - q) / b, where b = (1-X)/X, p = win rate, q = 1-p
   */
  private getKellyFraction(entryPrice: number, winRate: number): number {
    if (winRate < CONFIG.MIN_WIN_RATE_FOR_KELLY) return 0;

    const b = (1 - entryPrice) / entryPrice;
    const p = winRate;
    const q = 1 - p;
    const fullKelly = (b * p - q) / b;

    if (fullKelly <= 0) return 0;

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
