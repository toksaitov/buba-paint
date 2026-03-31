use thiserror::Error;

/// Unified error type for the buba-paint application.
///
/// Each variant wraps either a third-party error (via `#[from]`) or a
/// free-form `String` for domain-specific failures.
#[derive(Error, Debug)]
pub enum BubaError {
    /// Failure originating from `SQLite` / `rusqlite`.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Standard I/O error (file, network, etc.).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// `JSON` serialization / deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration-related error (missing env var, invalid value, etc.).
    #[error("config error: {0}")]
    Config(String),

    /// `WebSocket` / market-data feed error.
    #[error("feed error: {0}")]
    Feed(String),

    /// Backtesting engine error.
    #[error("backtest error: {0}")]
    Backtest(String),
}

/// Convenience alias so modules can write `errors::Result<T>` instead of
/// `std::result::Result<T, BubaError>`.
pub type Result<T> = std::result::Result<T, BubaError>;
