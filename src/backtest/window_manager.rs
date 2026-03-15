/// Pre-loads market windows from the data DB and advances through them.
///
/// Direct port of the TypeScript `WindowManager`.  Markets are loaded from the
/// `markets` table (which in the merged data DB includes `open_price`,
/// `close_price`, and `outcome` columns) and exposed as `MarketSettlement`s.
/// The `advance()` method is called on every tick to detect when the current
/// window opens or closes.
use std::str::FromStr;

use anyhow::Context;
use rusqlite::params;

use crate::types::{MarketWindow, SignalDirection};

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
        let mut stmt = conn
            .prepare(
                "SELECT market_id, question, up_token_id, down_token_id, \
                        condition_id, slug, start_time, end_time, \
                        open_price, close_price, outcome \
                 FROM markets \
                 WHERE end_time >= ?1 AND start_time <= ?2 \
                   AND outcome IS NOT NULL \
                 ORDER BY start_time",
            )
            .context("preparing markets query")?;

        // Timestamps are always positive, safe to cast
        #[allow(clippy::cast_possible_wrap)]
        let rows = stmt
            .query_map(params![start_time as i64, end_time as i64], |row| {
                let start_i64: i64 = row.get(6)?;
                let end_i64: i64 = row.get(7)?;
                let outcome_str: String = row.get(10)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    start_i64,
                    end_i64,
                    row.get::<_, f64>(8)?,
                    row.get::<_, f64>(9)?,
                    outcome_str,
                ))
            })
            .context("executing markets query")?;

        let mut windows = Vec::new();
        for row in rows {
            let (
                market_id,
                question,
                up_token_id,
                down_token_id,
                condition_id,
                slug,
                start_i64,
                end_i64,
                open_price,
                close_price,
                outcome_str,
            ) = row.context("reading market row")?;

            let outcome = SignalDirection::from_str(&outcome_str)
                .map_err(|e| anyhow::anyhow!("bad outcome for market {market_id}: {e}"))?;

            windows.push(MarketSettlement {
                market_id,
                question,
                up_token_id,
                down_token_id,
                condition_id,
                slug,
                start_time: start_i64 as u64,
                end_time: end_i64 as u64,
                open_price,
                close_price,
                outcome,
            });
        }

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

        // Check if the current window has ended.
        if let Some(ref current) = self.current {
            if timestamp >= current.end_time {
                events.closed = Some(current.clone());
                self.current = None;
            }
        }

        // Check if the next window has started (skip any already-expired).
        while self.current.is_none() && self.cursor < self.windows.len() {
            let next = &self.windows[self.cursor];
            if timestamp >= next.end_time {
                // Window already ended -- skip it.
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
        }
    }

    /// Reset to the initial state so the window sequence can be replayed.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.current = None;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/window_manager_tests.rs"]
mod tests;
