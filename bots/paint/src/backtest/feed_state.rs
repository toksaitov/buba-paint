/// Maintains market state during tick replay.
///
/// Accumulates Binance / Chainlink prices and `CLOB` order-book snapshots from
/// `TickGroup`s and exposes the latest view of the market.  Direct port of the
/// `TypeScript` `FeedState` class.
use crate::backtest::tick_replay::TickGroup;
use crate::types::{BookState, TopOfBook};

#[derive(Debug, Default)]
pub struct FeedState {
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub book_state: BookState,
}

impl FeedState {
    /// Creates a new `FeedState`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a tick group, updating whichever fields are present.
    pub fn update(&mut self, group: &TickGroup) {
        if let Some(ref sample) = group.binance {
            if let Some(price) = sample.price {
                self.binance_price = Some(price);
            }
        }

        if let Some(ref sample) = group.chainlink {
            if let Some(price) = sample.price {
                self.chainlink_price = Some(price);
            }
        }

        if let Some(ref sample) = group.clob_up {
            if let (Some(bid), Some(ask)) = (sample.bid, sample.ask) {
                self.book_state.up = Some(TopOfBook {
                    best_bid: bid,
                    best_ask: ask,
                    bid_size: sample.bid_size.unwrap_or(0.0),
                    ask_size: sample.ask_size.unwrap_or(0.0),
                    timestamp: group.timestamp,
                });
            }
        }

        if let Some(ref sample) = group.clob_down {
            if let (Some(bid), Some(ask)) = (sample.bid, sample.ask) {
                self.book_state.down = Some(TopOfBook {
                    best_bid: bid,
                    best_ask: ask,
                    bid_size: sample.bid_size.unwrap_or(0.0),
                    ask_size: sample.ask_size.unwrap_or(0.0),
                    timestamp: group.timestamp,
                });
            }
        }
    }

    /// Reset all state to the initial defaults.
    pub fn reset(&mut self) {
        self.binance_price = None;
        self.chainlink_price = None;
        self.book_state = BookState::default();
    }
}

#[cfg(test)]
#[path = "tests/feed_state_tests.rs"]
mod tests;
