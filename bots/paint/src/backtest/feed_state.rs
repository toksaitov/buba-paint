use crate::backtest::tick_replay::TickGroup;
use crate::signal_features::SignalState;

#[derive(Debug, Default)]
pub struct FeedState {
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub book_state: crate::types::BookState,
    pub signal_state: SignalState,
}

impl FeedState {
    /// Create an empty replay feed-state container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one replay group to the shared signal state.
    pub fn update(&mut self, group: &TickGroup) {
        if let Some(ref sample) = group.binance {
            if let Some(price) = sample.price {
                self.binance_price = Some(price);
                self.signal_state.update_binance_trade(
                    price,
                    0.0,
                    None,
                    group.timestamp,
                    group.timestamp_us,
                );
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
    }
}

#[cfg(test)]
#[path = "tests/feed_state_tests.rs"]
mod tests;
