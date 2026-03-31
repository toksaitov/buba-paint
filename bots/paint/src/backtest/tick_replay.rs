/// Loads ticks from `SQLite` and yields them grouped by 10ms windows.
///
/// Direct port of the `TypeScript` `TickReplay` class.  Each call to `next_group()`
/// returns a `TickGroup` that merges all ticks within a 10ms tolerance of the
/// first tick in the batch, keyed by source (`binance`, `chainlink`, `clob_up`,
/// `clob_down`).
use anyhow::Context;
use rusqlite::params;
use std::str::FromStr;
use std::sync::Arc;

use crate::types::ReplayFidelity;

/// A single raw row from `tick_data`.
#[derive(Debug, Clone)]
pub struct RawTick {
    pub timestamp: u64,
    pub source: String,
    pub event_type: String,
    pub market_id: Option<String>,
    pub asset_id: Option<String>,
    pub price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    pub fidelity: ReplayFidelity,
}

/// One source's snapshot at a given timestamp.
#[derive(Debug, Clone)]
pub struct TickSample {
    pub price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
}

/// All sources sampled at (approximately) the same timestamp.
#[derive(Debug, Clone)]
pub struct TickGroup {
    pub timestamp: u64,
    pub binance: Option<TickSample>,
    pub chainlink: Option<TickSample>,
    pub clob_up: Option<TickSample>,
    pub clob_down: Option<TickSample>,
    pub fidelity: ReplayFidelity,
}

pub type SharedTicks = Arc<Vec<RawTick>>;

/// Replays ticks from a shared in-memory tick buffer.
pub struct TickReplay {
    ticks: SharedTicks,
    cursor: usize,
}

impl TickReplay {
    /// Load ticks directly from a database connection for the given time range.
    pub fn from_db(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Self> {
        let ticks = Self::load_ticks(conn, start_time, end_time)?;
        Ok(Self::from_cached(ticks))
    }

    /// Build from a shared in-memory tick buffer (useful for sweep caching).
    pub fn from_cached<T>(ticks: T) -> Self
    where
        T: Into<SharedTicks>,
    {
        Self {
            ticks: ticks.into(),
            cursor: 0,
        }
    }

    /// Static helper: load ticks from a database connection without
    /// constructing a replay instance.  Useful for caching the tick vector
    /// across multiple sweep iterations.
    pub fn load_ticks(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Vec<RawTick>> {
        let start_ms = timestamp_param(start_time, "start_time")?;
        let end_ms = timestamp_param(end_time, "end_time")?;
        if conn.prepare("SELECT id FROM feed_events LIMIT 0").is_ok() {
            let feed_event_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM feed_events WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
                params![start_ms, end_ms],
                |row| row.get(0),
            )?;
            if feed_event_count > 0 {
                let mut stmt = conn
                    .prepare(
                        "SELECT received_at_ms, source, event_type, market_id, asset_id, price,
                            best_bid, best_ask, bid_size, ask_size, fidelity
                     FROM feed_events
                     WHERE received_at_ms >= ?1 AND received_at_ms <= ?2
                     ORDER BY received_at_ms, id",
                    )
                    .context("preparing feed_events query")?;

                let rows = stmt
                    .query_map(params![start_ms, end_ms], |row| {
                        let ts_i64: i64 = row.get(0)?;
                        let fidelity_str: String = row.get(10)?;
                        let fidelity = ReplayFidelity::from_str(&fidelity_str).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                rusqlite::types::Type::Text,
                                Box::from(e),
                            )
                        })?;
                        Ok(RawTick {
                            timestamp: ts_i64 as u64,
                            source: row.get(1)?,
                            event_type: row.get(2)?,
                            market_id: row.get(3)?,
                            asset_id: row.get(4)?,
                            price: row.get(5)?,
                            bid: row.get(6)?,
                            ask: row.get(7)?,
                            bid_size: row.get(8)?,
                            ask_size: row.get(9)?,
                            fidelity,
                        })
                    })
                    .context("executing feed_events query")?;

                let mut ticks = Vec::new();
                for row in rows {
                    ticks.push(row.context("reading feed_event row")?);
                }
                return Ok(ticks);
            }
        }

        let mut stmt = conn
            .prepare(
                "SELECT timestamp, source, price, bid, ask, bid_size, ask_size \
                 FROM tick_data WHERE timestamp >= ?1 AND timestamp <= ?2 \
                 ORDER BY timestamp",
            )
            .context("preparing tick_data query")?;

        let rows = stmt
            .query_map(params![start_ms, end_ms], |row| {
                let ts_i64: i64 = row.get(0)?;
                Ok(RawTick {
                    timestamp: ts_i64 as u64,
                    source: row.get(1)?,
                    event_type: "legacy_snapshot".to_string(),
                    market_id: None,
                    asset_id: None,
                    price: row.get(2)?,
                    bid: row.get(3)?,
                    ask: row.get(4)?,
                    bid_size: row.get(5)?,
                    ask_size: row.get(6)?,
                    fidelity: ReplayFidelity::LegacySnapshot,
                })
            })
            .context("executing tick_data query")?;

        let mut ticks = Vec::new();
        for row in rows {
            ticks.push(row.context("reading tick row")?);
        }
        Ok(ticks)
    }

    /// Total number of raw ticks loaded.
    pub fn total_ticks(&self) -> usize {
        self.ticks.len()
    }

    /// Reset the cursor to the beginning so the replay can be run again.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Yield the next tick group.  Returns `None` when exhausted.
    ///
    /// Groups all ticks within 10ms of the first tick's timestamp, matching
    /// the `TypeScript` implementation exactly:
    /// ```text
    /// while cursor < ticks.len && ticks[cursor].timestamp - ts <= 10
    /// ```
    pub fn next_group(&mut self) -> Option<TickGroup> {
        if self.cursor >= self.ticks.len() {
            return None;
        }

        let ts = self.ticks[self.cursor].timestamp;
        let mut group = TickGroup {
            timestamp: ts,
            binance: None,
            chainlink: None,
            clob_up: None,
            clob_down: None,
            fidelity: self.ticks[self.cursor].fidelity,
        };

        let group_window_ms = match self.ticks[self.cursor].fidelity {
            ReplayFidelity::RawEvent => 0,
            ReplayFidelity::LegacySnapshot => 10,
        };

        while self.cursor < self.ticks.len()
            && self.ticks[self.cursor].timestamp.saturating_sub(ts) <= group_window_ms
        {
            let tick = &self.ticks[self.cursor];
            let sample = TickSample {
                price: tick.price,
                bid: tick.bid,
                ask: tick.ask,
                bid_size: tick.bid_size,
                ask_size: tick.ask_size,
            };

            match tick.source.as_str() {
                "binance" => group.binance = Some(sample),
                "chainlink" => group.chainlink = Some(sample),
                "clob_up" => group.clob_up = Some(sample),
                "clob_down" => group.clob_down = Some(sample),
                _ => {}
            }

            self.cursor += 1;
        }

        Some(group)
    }
}

/// Convert a replay timestamp into the signed `SQLite` representation.
fn timestamp_param(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
}

#[cfg(test)]
#[path = "tests/tick_replay_tests.rs"]
mod tests;
