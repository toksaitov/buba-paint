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

use crate::db::clob_replay_blocks;
use crate::types::ReplayFidelity;

/// A single raw row from `tick_data`.
#[derive(Debug, Clone)]
pub struct RawTick {
    pub timestamp: u64,
    pub timestamp_us: Option<u64>,
    pub source: String,
    pub event_type: String,
    pub sequence_key: Option<String>,
    pub market_id: Option<String>,
    pub asset_id: Option<String>,
    pub price: Option<f64>,
    pub trade_size: Option<f64>,
    pub signed_quantity: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    pub depth_bid_notional: Option<f64>,
    pub depth_ask_notional: Option<f64>,
    pub depth_imbalance: Option<f64>,
    pub microprice: Option<f64>,
    pub fidelity: ReplayFidelity,
}

/// One replay row plus deterministic ordering metadata.
#[derive(Debug, Clone)]
struct ReplayRow {
    sort_us: u64,
    storage_order: u8,
    row_id: u64,
    tick: RawTick,
}

/// One source's snapshot at a given timestamp.
#[derive(Debug, Clone, Default)]
pub struct TickSample {
    pub event_type: String,
    pub sequence_key: Option<String>,
    pub price: Option<f64>,
    pub trade_size: Option<f64>,
    pub signed_quantity: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    pub depth_bid_notional: Option<f64>,
    pub depth_ask_notional: Option<f64>,
    pub depth_imbalance: Option<f64>,
    pub microprice: Option<f64>,
}

/// All sources sampled at (approximately) the same timestamp.
#[derive(Debug, Clone)]
pub struct TickGroup {
    pub timestamp: u64,
    pub timestamp_us: Option<u64>,
    pub binance: Option<TickSample>,
    pub chainlink: Option<TickSample>,
    pub clob_up: Option<TickSample>,
    pub clob_down: Option<TickSample>,
    pub fidelity: ReplayFidelity,
}

pub type SharedTicks = Arc<Vec<RawTick>>;

/// Return whether a table exposes the requested column.
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

/// Return a safe `SELECT` expression for an optional `feed_events` column.
fn optional_feed_event_column(conn: &rusqlite::Connection, column: &str) -> String {
    if has_column(conn, "feed_events", column) {
        column.to_string()
    } else {
        format!("NULL AS {column}")
    }
}

/// Return whether a table exists in the current connection.
fn has_table(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .is_ok_and(|count| count > 0)
}

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
    #[allow(clippy::too_many_lines)]
    pub fn load_ticks(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Vec<RawTick>> {
        let start_ms = timestamp_param(start_time, "start_time")?;
        let end_ms = timestamp_param(end_time, "end_time")?;
        let has_feed_events = conn.prepare("SELECT id FROM feed_events LIMIT 0").is_ok();
        let has_compact_clob = has_table(conn, "clob_replay_events");
        let has_clob_blocks = has_table(conn, "clob_replay_blocks");
        if has_feed_events || has_compact_clob || has_clob_blocks {
            let feed_event_count: i64 = if has_feed_events {
                conn.query_row(
                "SELECT COUNT(*) FROM feed_events WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
                params![start_ms, end_ms],
                |row| row.get(0),
                )?
            } else {
                0
            };
            let compact_clob_count: i64 = if has_compact_clob {
                conn.query_row(
                    "SELECT COUNT(*) FROM clob_replay_events WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
                    params![start_ms, end_ms],
                    |row| row.get(0),
                )?
            } else {
                0
            };
            let clob_block_count: i64 = if has_clob_blocks {
                conn.query_row(
                    "SELECT COALESCE(SUM(row_count), 0)
                     FROM clob_replay_blocks
                     WHERE max_received_at_ms >= ?1 AND min_received_at_ms <= ?2",
                    params![start_ms, end_ms],
                    |row| row.get(0),
                )?
            } else {
                0
            };
            if feed_event_count + compact_clob_count + clob_block_count > 0 {
                let mut ticks = Vec::with_capacity(
                    usize::try_from(
                        feed_event_count
                            .saturating_add(compact_clob_count)
                            .saturating_add(clob_block_count),
                    )
                    .unwrap_or_default(),
                );
                let query = replay_query(conn, has_feed_events, has_compact_clob);
                if !query.is_empty() {
                    let mut stmt = conn
                        .prepare(&query)
                        .context("preparing feed_events query")?;
                    let rows = stmt
                        .query_map(params![start_ms, end_ms], decode_replay_row)
                        .context("executing feed_events query")?;
                    for row in rows {
                        ticks.push(row.context("reading feed_event row")?);
                    }
                }
                load_clob_block_rows(conn, start_ms, end_ms, &mut ticks)?;
                ticks.sort_by_key(|row| (row.sort_us, row.storage_order, row.row_id));
                return Ok(ticks.into_iter().map(|row| row.tick).collect());
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
                    timestamp_us: Some((ts_i64 as u64).saturating_mul(1_000)),
                    source: row.get(1)?,
                    event_type: "legacy_snapshot".to_string(),
                    sequence_key: None,
                    market_id: None,
                    asset_id: None,
                    price: row.get(2)?,
                    trade_size: None,
                    signed_quantity: None,
                    bid: row.get(3)?,
                    ask: row.get(4)?,
                    bid_size: row.get(5)?,
                    ask_size: row.get(6)?,
                    depth_bid_notional: None,
                    depth_ask_notional: None,
                    depth_imbalance: None,
                    microprice: None,
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

    /// Return the first replay tick timestamp inside a database interval.
    pub fn first_tick_timestamp(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Option<u64>> {
        let start_ms = timestamp_param(start_time, "start_time")?;
        let end_ms = timestamp_param(end_time, "end_time")?;
        let mut first_feed_timestamp: Option<i64> = None;
        if conn.prepare("SELECT id FROM feed_events LIMIT 0").is_ok() {
            first_feed_timestamp = conn
                .query_row(
                    "SELECT MIN(received_at_ms)
                     FROM feed_events
                     WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
                    params![start_ms, end_ms],
                    |row| row.get(0),
                )
                .context("finding first feed_event timestamp")?;
        }
        let mut first_clob_timestamp: Option<i64> = None;
        if has_table(conn, "clob_replay_events") {
            first_clob_timestamp = conn
                .query_row(
                    "SELECT MIN(received_at_ms)
                     FROM clob_replay_events
                     WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
                    params![start_ms, end_ms],
                    |row| row.get(0),
                )
                .context("finding first clob_replay_event timestamp")?;
        }
        let first_clob_block_timestamp = first_clob_block_timestamp(conn, start_ms, end_ms)?;
        if first_feed_timestamp.is_some()
            || first_clob_timestamp.is_some()
            || first_clob_block_timestamp.is_some()
        {
            return Ok(first_feed_timestamp
                .into_iter()
                .chain(first_clob_timestamp)
                .chain(first_clob_block_timestamp)
                .min()
                .map(|timestamp| timestamp as u64));
        }
        let Ok(mut stmt) = conn.prepare(
            "SELECT MIN(timestamp)
             FROM tick_data
             WHERE timestamp >= ?1 AND timestamp <= ?2",
        ) else {
            return Ok(None);
        };
        let tick_timestamp: Option<i64> = stmt
            .query_row(params![start_ms, end_ms], |row| row.get(0))
            .context("finding first tick_data timestamp")?;
        Ok(tick_timestamp.map(|timestamp| timestamp as u64))
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
        let ts_us = self.ticks[self.cursor]
            .timestamp_us
            .unwrap_or_else(|| ts.saturating_mul(1_000));
        let mut group = TickGroup {
            timestamp: ts,
            timestamp_us: Some(ts_us),
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

        let start_cursor = self.cursor;
        while self.cursor < self.ticks.len() {
            let within_window = match self.ticks[self.cursor].fidelity {
                ReplayFidelity::RawEvent => self.cursor == start_cursor,
                ReplayFidelity::LegacySnapshot => {
                    self.ticks[self.cursor].timestamp.saturating_sub(ts) <= group_window_ms
                }
            };
            if !within_window {
                break;
            }
            let tick = &self.ticks[self.cursor];
            let sample = TickSample {
                event_type: tick.event_type.clone(),
                sequence_key: tick.sequence_key.clone(),
                price: tick.price,
                trade_size: tick.trade_size,
                signed_quantity: tick.signed_quantity,
                bid: tick.bid,
                ask: tick.ask,
                bid_size: tick.bid_size,
                ask_size: tick.ask_size,
                depth_bid_notional: tick.depth_bid_notional,
                depth_ask_notional: tick.depth_ask_notional,
                depth_imbalance: tick.depth_imbalance,
                microprice: tick.microprice,
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

/// Decode one replay SQL row into a sorted replay row.
fn decode_replay_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplayRow> {
    let ts_i64: i64 = row.get(0)?;
    let timestamp = ts_i64 as u64;
    let timestamp_us: Option<u64> = row.get(11)?;
    let fidelity_str: String = row.get(10)?;
    let fidelity = ReplayFidelity::from_str(&fidelity_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::from(e))
    })?;
    let storage_order: i64 = row.get(20)?;
    let row_id: i64 = row.get(19)?;
    Ok(ReplayRow {
        sort_us: timestamp_us.unwrap_or_else(|| timestamp.saturating_mul(1_000)),
        storage_order: u8::try_from(storage_order).unwrap_or(u8::MAX),
        row_id: u64::try_from(row_id).unwrap_or_default(),
        tick: RawTick {
            timestamp,
            timestamp_us,
            source: row.get(1)?,
            event_type: row.get(2)?,
            sequence_key: row.get(12)?,
            market_id: row.get(3)?,
            asset_id: row.get(4)?,
            price: row.get(5)?,
            trade_size: row.get(13)?,
            signed_quantity: row.get(14)?,
            bid: row.get(6)?,
            ask: row.get(7)?,
            bid_size: row.get(8)?,
            ask_size: row.get(9)?,
            depth_bid_notional: row.get(15)?,
            depth_ask_notional: row.get(16)?,
            depth_imbalance: row.get(17)?,
            microprice: row.get(18)?,
            fidelity,
        },
    })
}

/// Load compressed CLOB blocks into sorted replay rows.
fn load_clob_block_rows(
    conn: &rusqlite::Connection,
    start_ms: i64,
    end_ms: i64,
    ticks: &mut Vec<ReplayRow>,
) -> anyhow::Result<()> {
    if !has_table(conn, "clob_replay_blocks") {
        return Ok(());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, checksum, payload
             FROM clob_replay_blocks
             WHERE max_received_at_ms >= ?1 AND min_received_at_ms <= ?2
             ORDER BY min_received_at_ms, id",
        )
        .context("preparing CLOB replay block query")?;
    let rows = stmt
        .query_map(params![start_ms, end_ms], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .context("executing CLOB replay block query")?;
    for row in rows {
        let (block_id, checksum, payload) = row.context("reading CLOB replay block row")?;
        let decoded = clob_replay_blocks::decode_payload(&payload, &checksum)
            .context("decoding CLOB replay block")?;
        for (index, event) in decoded.into_iter().enumerate() {
            let received_at_ms =
                i64::try_from(event.received_at_ms).context("CLOB block timestamp overflow")?;
            if received_at_ms < start_ms || received_at_ms > end_ms {
                continue;
            }
            let timestamp = event.received_at_ms;
            let timestamp_us = event.received_at_us;
            let block_id = u64::try_from(block_id).unwrap_or_default();
            let index = u64::try_from(index).unwrap_or_default();
            ticks.push(ReplayRow {
                sort_us: timestamp_us.unwrap_or_else(|| timestamp.saturating_mul(1_000)),
                storage_order: 2,
                row_id: block_id.saturating_mul(10_000_000).saturating_add(index),
                tick: RawTick {
                    timestamp,
                    timestamp_us,
                    source: event.source,
                    event_type: event.event_type,
                    sequence_key: event.sequence_key,
                    market_id: event.market_id,
                    asset_id: event.asset_id,
                    price: None,
                    trade_size: None,
                    signed_quantity: None,
                    bid: Some(event.best_bid),
                    ask: Some(event.best_ask),
                    bid_size: Some(event.bid_size),
                    ask_size: Some(event.ask_size),
                    depth_bid_notional: None,
                    depth_ask_notional: None,
                    depth_imbalance: None,
                    microprice: event.microprice,
                    fidelity: event.fidelity,
                },
            });
        }
    }
    Ok(())
}

/// Return the first CLOB block event timestamp in one interval.
fn first_clob_block_timestamp(
    conn: &rusqlite::Connection,
    start_ms: i64,
    end_ms: i64,
) -> anyhow::Result<Option<i64>> {
    if !has_table(conn, "clob_replay_blocks") {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare(
            "SELECT checksum, payload
             FROM clob_replay_blocks
             WHERE max_received_at_ms >= ?1 AND min_received_at_ms <= ?2
             ORDER BY min_received_at_ms, id",
        )
        .context("preparing first CLOB replay block timestamp query")?;
    let rows = stmt
        .query_map(params![start_ms, end_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .context("executing first CLOB replay block timestamp query")?;
    let mut first = None;
    for row in rows {
        let (checksum, payload) = row.context("reading first CLOB replay block row")?;
        let decoded = clob_replay_blocks::decode_payload(&payload, &checksum)
            .context("decoding first CLOB replay block")?;
        for event in decoded {
            let timestamp =
                i64::try_from(event.received_at_ms).context("CLOB block timestamp overflow")?;
            if timestamp >= start_ms && timestamp <= end_ms {
                first = Some(first.map_or(timestamp, |current: i64| current.min(timestamp)));
            }
        }
    }
    Ok(first)
}

/// Build the replay query for generic and compact feed-event storage.
fn replay_query(
    conn: &rusqlite::Connection,
    has_feed_events: bool,
    has_compact_clob: bool,
) -> String {
    let mut branches = Vec::new();
    if has_feed_events {
        branches.push(format!(
            "SELECT received_at_ms, source, event_type, market_id, asset_id, price,
                    best_bid, best_ask, bid_size, ask_size, fidelity, received_at_us,
                    {}, {}, {}, {}, {}, {}, {}, id AS row_id, 0 AS storage_order
             FROM feed_events
             WHERE received_at_ms >= ?1 AND received_at_ms <= ?2",
            optional_feed_event_column(conn, "sequence_key"),
            optional_feed_event_column(conn, "trade_size"),
            optional_feed_event_column(conn, "signed_quantity"),
            optional_feed_event_column(conn, "depth_bid_notional"),
            optional_feed_event_column(conn, "depth_ask_notional"),
            optional_feed_event_column(conn, "depth_imbalance"),
            optional_feed_event_column(conn, "microprice"),
        ));
    }
    if has_compact_clob {
        branches.push(
            "SELECT received_at_ms, source, event_type, market_id, asset_id, NULL AS price,
                    best_bid, best_ask, bid_size, ask_size, fidelity, received_at_us,
                    sequence_key, NULL AS trade_size, NULL AS signed_quantity,
                    NULL AS depth_bid_notional, NULL AS depth_ask_notional,
                    NULL AS depth_imbalance, microprice, id AS row_id, 1 AS storage_order
             FROM clob_replay_events
             WHERE received_at_ms >= ?1 AND received_at_ms <= ?2"
                .to_string(),
        );
    }
    if branches.is_empty() {
        return String::new();
    }
    format!(
        "SELECT received_at_ms, source, event_type, market_id, asset_id, price,
                best_bid, best_ask, bid_size, ask_size, fidelity, received_at_us,
                sequence_key, trade_size, signed_quantity, depth_bid_notional,
                depth_ask_notional, depth_imbalance, microprice, row_id, storage_order
         FROM ({})
         ORDER BY COALESCE(received_at_us, received_at_ms * 1000), storage_order, row_id",
        branches.join(" UNION ALL ")
    )
}

/// Convert a replay timestamp into the signed `SQLite` representation.
fn timestamp_param(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
}

#[cfg(test)]
#[path = "tests/tick_replay_tests.rs"]
mod tests;
