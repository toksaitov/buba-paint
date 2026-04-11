/// Pre-loads market windows from the data DB and advances through them.
///
/// Direct port of the `TypeScript` `WindowManager`.  Markets are loaded from the
/// `markets` table (which in the merged data DB includes `open_price`,
/// `close_price`, and `outcome` columns) and exposed as `MarketSettlement`s.
/// The `advance()` method is called on every tick to detect when the current
/// window opens or closes.
use std::str::FromStr;

use anyhow::Context;
use rusqlite::params;

use crate::types::{MarketWindow, SignalDirection};

/// Return whether the given table exposes the requested column.
fn has_column(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let pragma = format!("PRAGMA table_info({table})");
    let Ok(mut stmt) = conn.prepare(&pragma) else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    rows.flatten().any(|name| name == column)
}

/// Build the `SELECT` expression for an optional market column.
fn optional_market_column(conn: &rusqlite::Connection, column: &str) -> String {
    if has_column(conn, "markets", column) {
        column.to_string()
    } else {
        format!("NULL AS {column}")
    }
}

/// Convert a replay timestamp into the signed `SQLite` representation.
fn timestamp_param(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
}

/// A fully-settled market window with its outcome.
#[derive(Debug, Clone)]
pub struct MarketSettlement {
    pub market_id: String,
    pub question: String,
    pub up_token_id: String,
    pub down_token_id: String,
    pub condition_id: String,
    pub slug: String,
    pub start_time: u64,
    pub end_time: u64,
    pub open_price: f64,
    pub close_price: f64,
    pub outcome: SignalDirection,
    pub resolution_source: Option<String>,
    pub fee_profile: Option<String>,
    pub order_min_size: Option<f64>,
    pub order_price_min_tick_size: Option<f64>,
    pub maker_base_fee: Option<f64>,
    pub taker_base_fee: Option<f64>,
    pub rewards_min_size: Option<f64>,
    pub rewards_max_spread: Option<f64>,
    pub fees_enabled: Option<bool>,
    pub fee_schedule_json: Option<String>,
    pub token_fee_rates_json: Option<String>,
    pub accepting_orders: Option<bool>,
    pub accepting_orders_timestamp: Option<String>,
    pub clear_book_on_start: Option<bool>,
}

/// Events returned by `advance()` indicating what changed at a given
/// timestamp.
pub struct WindowEvents {
    pub opened: Option<MarketSettlement>,
    pub closed: Option<MarketSettlement>,
}

/// Manages a sorted list of market windows and tracks which one is "current".
pub struct WindowManager {
    windows: Vec<MarketSettlement>,
    cursor: usize,
    pub current: Option<MarketSettlement>,
}

impl WindowManager {
    /// Load all settled windows from the database that overlap with the
    /// requested time range.
    pub fn new(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Self> {
        let windows = load_settlements(conn, start_time, end_time)?;

        Ok(Self {
            windows,
            cursor: 0,
            current: None,
        })
    }

    /// Create from pre-loaded settlements (for testing / internal use).
    pub fn from_settlements(settlements: Vec<MarketSettlement>) -> Self {
        Self {
            windows: settlements,
            cursor: 0,
            current: None,
        }
    }

    /// Total number of windows loaded.
    pub fn total_windows(&self) -> usize {
        self.windows.len()
    }

    /// Advance to the given timestamp.  Returns events indicating whether a
    /// market window opened or closed (or both) at this point.
    pub fn advance(&mut self, timestamp: u64) -> WindowEvents {
        let mut events = WindowEvents {
            opened: None,
            closed: None,
        };

        if let Some(ref current) = self.current {
            if timestamp >= current.end_time {
                events.closed = Some(current.clone());
                self.current = None;
            }
        }

        while self.current.is_none() && self.cursor < self.windows.len() {
            let next = &self.windows[self.cursor];
            if timestamp >= next.end_time {
                self.cursor += 1;
                continue;
            }
            if timestamp >= next.start_time {
                let window = self.windows[self.cursor].clone();
                self.cursor += 1;
                events.opened = Some(window.clone());
                self.current = Some(window);
            }
            break;
        }

        events
    }

    /// Convert a `MarketSettlement` into the runtime `MarketWindow` type.
    pub fn to_market_window(settlement: &MarketSettlement) -> MarketWindow {
        MarketWindow {
            market_id: settlement.market_id.clone(),
            question: settlement.question.clone(),
            up_token_id: settlement.up_token_id.clone(),
            down_token_id: settlement.down_token_id.clone(),
            condition_id: settlement.condition_id.clone(),
            start_time: settlement.start_time,
            end_time: settlement.end_time,
            slug: settlement.slug.clone(),
            outcome: Some(settlement.outcome.to_string()),
            resolution_source: settlement.resolution_source.clone(),
            fee_profile: settlement.fee_profile.clone(),
            order_min_size: settlement.order_min_size,
            order_price_min_tick_size: settlement.order_price_min_tick_size,
            maker_base_fee: settlement.maker_base_fee,
            taker_base_fee: settlement.taker_base_fee,
            rewards_min_size: settlement.rewards_min_size,
            rewards_max_spread: settlement.rewards_max_spread,
            fees_enabled: settlement.fees_enabled,
            fee_schedule_json: settlement.fee_schedule_json.clone(),
            token_fee_rates_json: settlement.token_fee_rates_json.clone(),
            accepting_orders: settlement.accepting_orders,
            accepting_orders_timestamp: settlement.accepting_orders_timestamp.clone(),
            clear_book_on_start: settlement.clear_book_on_start,
        }
    }

    /// Reset to the initial state so the window sequence can be replayed.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.current = None;
    }
}

/// Load settled market windows that overlap with the requested replay interval.
fn load_settlements(
    conn: &rusqlite::Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<Vec<MarketSettlement>> {
    let sql = format!(
        "SELECT market_id, question, up_token_id, down_token_id, \
                condition_id, slug, start_time, end_time, \
                open_price, close_price, outcome, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {} \
         FROM markets \
         WHERE end_time >= ?1 AND start_time <= ?2 \
           AND outcome IS NOT NULL \
         ORDER BY start_time",
        optional_market_column(conn, "resolution_source"),
        optional_market_column(conn, "fee_profile"),
        optional_market_column(conn, "order_min_size"),
        optional_market_column(conn, "order_price_min_tick_size"),
        optional_market_column(conn, "maker_base_fee"),
        optional_market_column(conn, "taker_base_fee"),
        optional_market_column(conn, "rewards_min_size"),
        optional_market_column(conn, "rewards_max_spread"),
        optional_market_column(conn, "fees_enabled"),
        optional_market_column(conn, "fee_schedule_json"),
        optional_market_column(conn, "token_fee_rates_json"),
        optional_market_column(conn, "accepting_orders"),
        optional_market_column(conn, "accepting_orders_timestamp"),
        optional_market_column(conn, "clear_book_on_start"),
    );
    let mut stmt = conn.prepare(&sql).context("preparing markets query")?;
    let start_ms = timestamp_param(start_time, "start_time")?;
    let end_ms = timestamp_param(end_time, "end_time")?;
    let rows = stmt
        .query_map(params![start_ms, end_ms], read_market_settlement_row)
        .context("executing markets query")?;

    let mut windows = Vec::new();
    for row in rows {
        windows.push(row.context("reading market row")?);
    }
    Ok(windows)
}

/// Convert a `SQLite` market row into a fully populated settlement.
fn read_market_settlement_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarketSettlement> {
    let market_id: String = row.get(0)?;
    let outcome_value: String = row.get(10)?;
    let outcome = parse_market_outcome(&market_id, &outcome_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            10,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    Ok(MarketSettlement {
        market_id,
        question: row.get(1)?,
        up_token_id: row.get(2)?,
        down_token_id: row.get(3)?,
        condition_id: row.get(4)?,
        slug: row.get(5)?,
        start_time: row.get::<_, i64>(6)? as u64,
        end_time: row.get::<_, i64>(7)? as u64,
        open_price: row.get::<_, Option<f64>>(8)?.unwrap_or(0.0),
        close_price: row.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
        outcome,
        resolution_source: row.get(11)?,
        fee_profile: row.get(12)?,
        order_min_size: row.get(13)?,
        order_price_min_tick_size: row.get(14)?,
        maker_base_fee: row.get(15)?,
        taker_base_fee: row.get(16)?,
        rewards_min_size: row.get(17)?,
        rewards_max_spread: row.get(18)?,
        fees_enabled: row.get(19)?,
        fee_schedule_json: row.get(20)?,
        token_fee_rates_json: row.get(21)?,
        accepting_orders: row.get(22)?,
        accepting_orders_timestamp: row.get(23)?,
        clear_book_on_start: row.get(24)?,
    })
}

/// Parse a stored market outcome string into the internal enum.
fn parse_market_outcome(market_id: &str, outcome: &str) -> anyhow::Result<SignalDirection> {
    SignalDirection::from_str(outcome)
        .map_err(|error| anyhow::anyhow!("bad outcome for market {market_id}: {error}"))
}

#[cfg(test)]
#[path = "tests/window_manager_tests.rs"]
mod tests;
