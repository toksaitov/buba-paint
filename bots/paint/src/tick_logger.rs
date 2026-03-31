use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{error, info};

use crate::db::database::Database;
use crate::types::{BookState, TopOfBook};

/// A single tick entry ready to be persisted.
#[derive(Debug, Clone)]
pub(crate) struct TickEntry {
    pub source: &'static str,
    pub price: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
}

/// Shared state updated by the main loop; the tick logger task reads it
/// periodically and persists snapshots to the database.
#[derive(Debug, Default)]
pub struct TickLoggerState {
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub book_state: BookState,
}

/// Build the list of tick entries from the current logger state.
///
/// Pure function -- no I/O, no database access.  Returns up to four entries
/// (binance, chainlink, `clob_up`, `clob_down`) depending on which data is
/// present.
pub(crate) fn build_tick_entries(state: &TickLoggerState) -> Vec<TickEntry> {
    let mut entries = Vec::with_capacity(4);

    if let Some(price) = state.binance_price {
        entries.push(TickEntry {
            source: "binance",
            price: Some(price),
            best_bid: None,
            best_ask: None,
            bid_size: None,
            ask_size: None,
        });
    }

    if let Some(price) = state.chainlink_price {
        entries.push(TickEntry {
            source: "chainlink",
            price: Some(price),
            best_bid: None,
            best_ask: None,
            bid_size: None,
            ask_size: None,
        });
    }

    if let Some(ref up) = state.book_state.up {
        entries.push(book_entry("clob_up", up));
    }

    if let Some(ref down) = state.book_state.down {
        entries.push(book_entry("clob_down", down));
    }

    entries
}

/// Book entry.
fn book_entry(source: &'static str, book: &TopOfBook) -> TickEntry {
    TickEntry {
        source,
        price: None,
        best_bid: Some(book.best_bid),
        best_ask: Some(book.best_ask),
        bid_size: Some(book.bid_size),
        ask_size: Some(book.ask_size),
    }
}

/// Run the periodic tick logger.
///
/// Opens its own `SQLite` connection (rusqlite is `!Sync` so the DB cannot be
/// shared via `Arc` across threads).  Every `tick_interval_ms` ms it reads
/// the latest prices and book state from `state` and writes up to four tick
/// rows (binance, `clob_up`, `clob_down`, chainlink) into the database.
///
/// This function runs forever (or until cancelled).
pub async fn run_tick_logger(
    db_path: String,
    tick_interval_ms: u64,
    state: Arc<RwLock<TickLoggerState>>,
) {
    let db = match Database::new(&db_path) {
        Ok(db) => db,
        Err(e) => {
            error!(logger = "tick", "failed to open database: {e}");
            return;
        }
    };

    let tick_ms = tick_interval_ms;
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));

    info!(tick_interval_ms = tick_ms, "tick logger started");

    loop {
        interval.tick().await;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let snapshot = {
            let guard = state.read().await;
            TickLoggerState {
                binance_price: guard.binance_price,
                chainlink_price: guard.chainlink_price,
                book_state: guard.book_state.clone(),
            }
        };

        let entries = build_tick_entries(&snapshot);

        for entry in &entries {
            if let Err(e) = db.log_tick(
                now_ms,
                entry.source,
                entry.price,
                entry.best_bid,
                entry.best_ask,
                entry.bid_size,
                entry.ask_size,
            ) {
                error!(
                    logger = "tick",
                    source = entry.source,
                    "failed to log tick: {e}"
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/tick_logger_tests.rs"]
mod tests;
