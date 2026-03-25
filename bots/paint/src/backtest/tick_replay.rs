/// Loads ticks from `SQLite` and yields them grouped by 10ms windows.
///
/// Direct port of the TypeScript `TickReplay` class.  Each call to `next_group()`
/// returns a `TickGroup` that merges all ticks within a 10ms tolerance of the
/// first tick in the batch, keyed by source (`binance`, `chainlink`, `clob_up`,
/// `clob_down`).
use anyhow::Context;
use rusqlite::params;

/// A single raw row from `tick_data`.
#[derive(Debug, Clone)]
pub struct RawTick {
    pub timestamp: u64,
    pub source: String,
    pub price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
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
}

/// Replays ticks from a pre-loaded `Vec<RawTick>`.
pub struct TickReplay {
    ticks: Vec<RawTick>,
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
        Ok(Self { ticks, cursor: 0 })
    }

    /// Build from a pre-loaded tick vector (useful for sweep caching).
    pub fn from_cached(ticks: Vec<RawTick>) -> Self {
        Self { ticks, cursor: 0 }
    }

    /// Static helper: load ticks from a database connection without
    /// constructing a replay instance.  Useful for caching the tick vector
    /// across multiple sweep iterations.
    pub fn load_ticks(
        conn: &rusqlite::Connection,
        start_time: u64,
        end_time: u64,
    ) -> anyhow::Result<Vec<RawTick>> {
        let mut stmt = conn
            .prepare(
                "SELECT timestamp, source, price, bid, ask, bid_size, ask_size \
                 FROM tick_data WHERE timestamp >= ?1 AND timestamp <= ?2 \
                 ORDER BY timestamp",
            )
            .context("preparing tick_data query")?;

        // Timestamps are always positive, safe to cast
        #[allow(clippy::cast_possible_wrap)]
        let rows = stmt
            .query_map(params![start_time as i64, end_time as i64], |row| {
                let ts_i64: i64 = row.get(0)?;
                Ok(RawTick {
                    timestamp: ts_i64 as u64,
                    source: row.get(1)?,
                    price: row.get(2)?,
                    bid: row.get(3)?,
                    ask: row.get(4)?,
                    bid_size: row.get(5)?,
                    ask_size: row.get(6)?,
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
    /// the TypeScript implementation exactly:
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
        };

        while self.cursor < self.ticks.len() && self.ticks[self.cursor].timestamp - ts <= 10 {
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
                _ => {} // ignore unknown sources
            }

            self.cursor += 1;
        }

        Some(group)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/tick_replay_tests.rs"]
mod tests;
