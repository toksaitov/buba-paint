use crate::backtest::tick_replay::TickGroup;
use crate::signal_features::SignalState;
use crate::types::OrderLevel;

#[derive(Debug, Default)]
pub struct FeedState {
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub book_state: crate::types::BookState,
    pub signal_state: SignalState,
    binance_book_from_book_ticker: bool,
}

impl FeedState {
    /// Create an empty replay feed-state container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one replay group to the shared signal state.
    pub fn update(&mut self, group: &TickGroup) {
        if let Some(ref sample) = group.binance {
            match sample.event_type.as_str() {
                "aggTrade" => {
                    if let Some(price) = sample.price {
                        self.binance_price = Some(price);
                        self.signal_state.update_binance_trade(
                            price,
                            sample.trade_size.unwrap_or(0.0),
                            sample.signed_quantity,
                            group.timestamp,
                            group.timestamp_us,
                        );
                    }
                }
                "bookTicker" => {
                    update_binance_book(&mut self.signal_state, sample, group.timestamp);
                    self.binance_book_from_book_ticker = true;
                }
                "depth" => {
                    if !self.binance_book_from_book_ticker {
                        update_binance_book(&mut self.signal_state, sample, group.timestamp);
                    }
                    update_binance_depth(&mut self.signal_state, sample, group.timestamp);
                }
                _ => {
                    if let Some(price) = sample.price {
                        self.binance_price = Some(price);
                        self.signal_state.update_binance_trade(
                            price,
                            sample.trade_size.unwrap_or(0.0),
                            sample.signed_quantity,
                            group.timestamp,
                            group.timestamp_us,
                        );
                    }
                    update_binance_book(&mut self.signal_state, sample, group.timestamp);
                }
            }
        }

        if let Some(ref sample) = group.chainlink {
            if let Some(price) = sample.price {
                self.chainlink_price = Some(price);
                self.signal_state
                    .update_chainlink(price, group.timestamp, group.timestamp_us);
            }
        }

        if group.clob_up.is_some() || group.clob_down.is_some() {
            let mut book_state = self.book_state.clone();
            if let Some(ref sample) = group.clob_up {
                if let (Some(bid), Some(ask)) = (sample.bid, sample.ask) {
                    book_state.up = Some(crate::types::TopOfBook {
                        best_bid: bid,
                        best_ask: ask,
                        bid_size: sample.bid_size.unwrap_or(0.0),
                        ask_size: sample.ask_size.unwrap_or(0.0),
                        timestamp: group.timestamp,
                        observed_at_ms: group.timestamp,
                    });
                }
            }
            if let Some(ref sample) = group.clob_down {
                if let (Some(bid), Some(ask)) = (sample.bid, sample.ask) {
                    book_state.down = Some(crate::types::TopOfBook {
                        best_bid: bid,
                        best_ask: ask,
                        bid_size: sample.bid_size.unwrap_or(0.0),
                        ask_size: sample.ask_size.unwrap_or(0.0),
                        timestamp: group.timestamp,
                        observed_at_ms: group.timestamp,
                    });
                }
            }
            self.book_state = book_state.clone();
            self.signal_state
                .update_clob(book_state, group.timestamp, group.timestamp_us);
        }
    }

    /// Reset the replay market state to its initial default.
    pub fn reset(&mut self) {
        self.binance_price = None;
        self.chainlink_price = None;
        self.book_state = crate::types::BookState::default();
        self.signal_state.reset();
        self.binance_book_from_book_ticker = false;
    }
}

/// Apply one replayed Binance top-of-book sample to signal state.
fn update_binance_book(
    signal_state: &mut SignalState,
    sample: &crate::backtest::tick_replay::TickSample,
    timestamp: u64,
) {
    let (Some(best_bid), Some(best_ask)) = (sample.bid, sample.ask) else {
        return;
    };
    signal_state.update_binance_book(
        best_bid,
        best_ask,
        sample.bid_size.unwrap_or(0.0),
        sample.ask_size.unwrap_or(0.0),
        timestamp,
        sample.sequence_key.clone(),
    );
}

/// Apply one replayed Binance depth summary to signal state.
fn update_binance_depth(
    signal_state: &mut SignalState,
    sample: &crate::backtest::tick_replay::TickSample,
    timestamp: u64,
) {
    let (Some(best_bid), Some(best_ask)) = (sample.bid, sample.ask) else {
        return;
    };
    let bid_size = replay_depth_size(sample.bid_size, sample.depth_bid_notional, best_bid);
    let ask_size = replay_depth_size(sample.ask_size, sample.depth_ask_notional, best_ask);
    signal_state.update_binance_depth(
        vec![OrderLevel {
            price: best_bid,
            size: bid_size,
        }],
        vec![OrderLevel {
            price: best_ask,
            size: ask_size,
        }],
        timestamp,
        sample.sequence_key.clone(),
    );
}

/// Return a usable replay depth size from top-level size or notional.
fn replay_depth_size(size: Option<f64>, notional: Option<f64>, price: f64) -> f64 {
    if let Some(size) = size
        && size > 0.0
    {
        return size;
    }
    if price > 0.0 {
        return notional.map_or(0.0, |value| value / price).max(0.0);
    }
    0.0
}

#[cfg(test)]
#[path = "tests/feed_state_tests.rs"]
mod tests;
