// === Feed Events ===

export interface BinanceTick {
  eventTime: number;
  price: number;
  quantity: number;
  tradeTime: number;
}

export interface OrderLevel {
  price: number;
  size: number;
}

export interface ClobBookSnapshot {
  assetId: string;
  market: string;
  timestamp: number;
  bids: OrderLevel[];
  asks: OrderLevel[];
}

export interface ClobPriceChange {
  assetId: string;
  market: string;
  timestamp: number;
  changes: PriceChangeEntry[];
}

export interface PriceChangeEntry {
  assetId: string;
  price: number;
  size: number;
  side: "BUY" | "SELL";
}

export interface ChainlinkTick {
  symbol: string;
  timestamp: number;
  value: number;
}

// === Top-of-Book State ===

export interface TopOfBook {
  bestBid: number;
  bestAsk: number;
  bidSize: number;
  askSize: number;
  timestamp: number;
}

export interface BookState {
  up: TopOfBook | null;
  down: TopOfBook | null;
}

// === Market Discovery ===

export interface GammaMarket {
  id: string;
  question: string;
  conditionId: string;
  slug: string;
  active: boolean;
  closed: boolean;
  acceptingOrders: boolean;
  outcomes: string[];
  outcomePrices: number[];
  clobTokenIds: string[];
  orderPriceMinTickSize: number;
  endDate: string;
  negRisk: boolean;
  negRiskMarketID: string;
}

export interface MarketWindow {
  marketId: string;
  question: string;
  upTokenId: string;
  downTokenId: string;
  conditionId: string;
  startTime: number;
  endTime: number;
  slug: string;
}

// === Strategies ===

export type SignalDirection = "UP" | "DOWN";

export interface Signal {
  timestamp: number;
  strategy: string;
  direction: SignalDirection;
  confidence: number;
  binancePrice: number;
  chainlinkPrice: number;
  upAsk: number;
  downAsk: number;
  upBid: number;
  downBid: number;
  metadata: Record<string, unknown>;
}

export interface StrategyContext {
  binancePrice: number;
  binanceMomentum: number;
  chainlinkPrice: number | null;
  bookState: BookState;
  windowTimeRemainingMs: number;
}

export interface Strategy {
  readonly name: string;
  evaluate(ctx: StrategyContext): Signal | Signal[] | null;
}

// === Position Manager ===

export type TradeStatus = "open" | "closed" | "expired";

export interface SimulatedTrade {
  id?: number;
  timestamp: number;
  marketId: string;
  strategy: string;
  side: SignalDirection;
  tokenId: string;
  entryPrice: number;
  size: number;
  status: TradeStatus;
}

export interface TradeResult {
  tradeId: number;
  exitPrice: number;
  settlementPrice: number;
  pnl0pct: number;
  pnl1pct: number;
  pnl2pct: number;
  pnl3pct: number;
}

// === Feed Base ===

export type FeedStatus = "disconnected" | "connecting" | "connected";
