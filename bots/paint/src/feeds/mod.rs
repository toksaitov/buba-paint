pub mod binance_feed;
pub mod chainlink_feed;
pub mod clob_feed;
pub(crate) mod util;

/// Messages sent from feeds to the main loop.
#[derive(Debug)]
pub enum FeedMessage {
    BinanceTick {
        price: f64,
        timestamp: u64,
        payload_json: Option<String>,
    },
    ChainlinkPrice {
        price: f64,
        timestamp: u64,
        payload_json: Option<String>,
    },
    ClobBook {
        book_state: crate::types::BookState,
        timestamp: u64,
        payload_json: Option<String>,
    },
    ClobPriceChange {
        book_state: crate::types::BookState,
        timestamp: u64,
        payload_json: Option<String>,
    },
    FeedConnected(String),
    FeedDisconnected(String),
    ChainlinkStale,
}
