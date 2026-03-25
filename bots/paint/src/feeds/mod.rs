pub mod binance_feed;
pub mod chainlink_feed;
pub mod clob_feed;
pub(crate) mod util;

/// Messages sent from feeds to the main loop.
#[derive(Debug)]
pub enum FeedMessage {
    BinanceTick { price: f64, timestamp: u64 },
    ChainlinkPrice { price: f64, timestamp: u64 },
    ClobBook { book_state: crate::types::BookState },
    ClobPriceChange { book_state: crate::types::BookState },
    FeedConnected(String),
    FeedDisconnected(String),
    ChainlinkStale,
}
