use std::collections::HashMap;

use crate::config::FeedEventStorageProfile;
use crate::types::{FeedEvent, OrderLevel};

const BINANCE_DEPTH_BUCKET_MS: u64 = 250;
#[derive(Debug, Clone, Copy, PartialEq)]
struct PersistedTopOfBook {
    best_bid: f64,
    best_ask: f64,
    bid_size: f64,
    ask_size: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct FeedEventStorageState {
    profile: FeedEventStorageProfile,
    binance_depth_buckets: HashMap<String, u64>,
    clob_top_of_book: HashMap<String, PersistedTopOfBook>,
    clob_book_connections: HashMap<String, String>,
    rows_by_source_event: HashMap<String, u64>,
}

impl FeedEventStorageState {
    /// Build one storage-state tracker for the selected live persistence profile.
    #[must_use]
    pub(crate) fn new(profile: FeedEventStorageProfile) -> Self {
        Self {
            profile,
            binance_depth_buckets: HashMap::new(),
            clob_top_of_book: HashMap::new(),
            clob_book_connections: HashMap::new(),
            rows_by_source_event: HashMap::new(),
        }
    }

    /// Return one trade event in the active storage profile.
    pub(crate) fn prepare_binance_trade(
        &mut self,
        mut event: FeedEvent,
        trade_size: f64,
        signed_quantity: Option<f64>,
    ) -> FeedEvent {
        event.trade_size = Some(trade_size);
        event.signed_quantity = signed_quantity;
        if self.profile == FeedEventStorageProfile::ReplayGrade {
            sanitize_replay_event(&mut event);
        } else if self.profile != FeedEventStorageProfile::FullDebug {
            sanitize_hot_event(&mut event);
        }
        event
    }

    /// Return one Binance top-of-book event for replay-capable storage profiles.
    pub(crate) fn prepare_binance_book_ticker(
        &mut self,
        mut event: FeedEvent,
    ) -> Option<FeedEvent> {
        match self.profile {
            FeedEventStorageProfile::Compact => None,
            FeedEventStorageProfile::ReplayGrade => {
                sanitize_replay_event(&mut event);
                Some(event)
            }
            FeedEventStorageProfile::FullDebug => Some(event),
        }
    }

    /// Return one bucketed Binance depth summary event.
    pub(crate) fn prepare_binance_depth(
        &mut self,
        mut event: FeedEvent,
        symbol: Option<&str>,
        bid_levels: &[OrderLevel],
        ask_levels: &[OrderLevel],
    ) -> Option<FeedEvent> {
        event.depth_bid_notional = Some(depth_notional(bid_levels));
        event.depth_ask_notional = Some(depth_notional(ask_levels));
        event.depth_imbalance =
            compute_depth_imbalance(event.depth_bid_notional, event.depth_ask_notional);
        event.microprice = compute_microprice(
            event.best_bid,
            event.best_ask,
            event.bid_size,
            event.ask_size,
        );
        if self.profile == FeedEventStorageProfile::FullDebug {
            return Some(event);
        }
        if self.profile == FeedEventStorageProfile::ReplayGrade {
            sanitize_replay_event(&mut event);
            return Some(event);
        }
        let symbol = symbol.unwrap_or("binance");
        let bucket = bucket_ms(event.received_at_ms, BINANCE_DEPTH_BUCKET_MS);
        if self.binance_depth_buckets.get(symbol).copied() == Some(bucket) {
            return None;
        }
        self.binance_depth_buckets
            .insert(symbol.to_string(), bucket);
        sanitize_replay_event(&mut event);
        Some(event)
    }

    /// Return one compact Chainlink price event.
    pub(crate) fn prepare_chainlink_price(&mut self, mut event: FeedEvent) -> FeedEvent {
        if self.profile != FeedEventStorageProfile::FullDebug {
            sanitize_replay_event(&mut event);
        }
        event
    }

    /// Return one compact `CLOB` top-of-book event when it is actionable.
    pub(crate) fn prepare_clob_top_of_book(&mut self, mut event: FeedEvent) -> Option<FeedEvent> {
        if self.profile == FeedEventStorageProfile::FullDebug {
            return Some(event);
        }
        let snapshot = persisted_top_of_book(&event)?;
        let key = clob_storage_key(&event);
        if let Some(previous) = self.clob_top_of_book.get(&key)
            && same_top_of_book(previous, &snapshot)
        {
            return None;
        }
        self.clob_top_of_book.insert(key, snapshot);
        event.microprice = compute_microprice(
            event.best_bid,
            event.best_ask,
            event.bid_size,
            event.ask_size,
        );
        sanitize_replay_event(&mut event);
        Some(event)
    }

    /// Return one compact `CLOB` bootstrap snapshot per connection and side.
    pub(crate) fn prepare_clob_book_snapshot(&mut self, mut event: FeedEvent) -> Option<FeedEvent> {
        event.microprice = compute_microprice(
            event.best_bid,
            event.best_ask,
            event.bid_size,
            event.ask_size,
        );
        if self.profile == FeedEventStorageProfile::FullDebug {
            return Some(event);
        }
        let key = clob_storage_key(&event);
        let connection_id = event.connection_id.clone().unwrap_or_default();
        if self.clob_book_connections.get(&key) == Some(&connection_id) {
            return None;
        }
        self.clob_book_connections.insert(key, connection_id);
        sanitize_replay_event(&mut event);
        Some(event)
    }

    /// Return one compact metadata event.
    pub(crate) fn prepare_clob_meta(&mut self, mut event: FeedEvent) -> Option<FeedEvent> {
        if event.event_type == "last_trade_price"
            && self.profile != FeedEventStorageProfile::FullDebug
        {
            return None;
        }
        if self.profile == FeedEventStorageProfile::ReplayGrade {
            sanitize_replay_event(&mut event);
        } else if self.profile != FeedEventStorageProfile::FullDebug {
            event.payload_json = None;
        }
        Some(event)
    }

    /// Count one accepted row by source and event type.
    pub(crate) fn record_enqueued_key(&mut self, source: &str, event_type: &str) {
        let key = format!("{source}:{event_type}");
        *self.rows_by_source_event.entry(key).or_insert(0) += 1;
    }

    /// Drain the grouped row counters accumulated since the last report.
    pub(crate) fn take_row_counts(&mut self) -> Vec<(String, u64)> {
        let mut rows = self.rows_by_source_event.drain().collect::<Vec<_>>();
        rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        rows
    }
}

/// Zero out bulky payload fields on compact hot-path events.
fn sanitize_hot_event(event: &mut FeedEvent) {
    event.payload_json = None;
    event.details_json = None;
    event.source_topic = None;
    event.connection_id = None;
    event.sequence_key = None;
    if event.source == "binance" || event.source == "chainlink" {
        event.market_id = None;
    }
}

/// Strip bulky payloads while preserving replay-critical sequencing fields.
fn sanitize_replay_event(event: &mut FeedEvent) {
    event.payload_json = None;
    event.details_json = None;
    if event.source == "binance" || event.source == "chainlink" {
        event.market_id = None;
    }
}

/// Bucket one timestamp by a fixed window size.
fn bucket_ms(timestamp_ms: u64, width_ms: u64) -> u64 {
    timestamp_ms / width_ms
}

/// Sum the notional represented by one shallow depth side.
fn depth_notional(levels: &[OrderLevel]) -> f64 {
    levels.iter().map(|level| level.price * level.size).sum()
}

/// Compute a normalized notional imbalance from bid and ask depth.
fn compute_depth_imbalance(bid_notional: Option<f64>, ask_notional: Option<f64>) -> Option<f64> {
    let bid_notional = bid_notional?;
    let ask_notional = ask_notional?;
    let total = bid_notional + ask_notional;
    if total <= 0.0 {
        return None;
    }
    Some((bid_notional - ask_notional) / total)
}

/// Compute a microprice from the current top-of-book.
fn compute_microprice(
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
) -> Option<f64> {
    let best_bid = best_bid?;
    let best_ask = best_ask?;
    let bid_size = bid_size?;
    let ask_size = ask_size?;
    let total_size = bid_size + ask_size;
    if total_size <= 0.0 {
        return None;
    }
    Some(((best_ask * bid_size) + (best_bid * ask_size)) / total_size)
}

/// Build one stable per-side storage key for compact `CLOB` rows.
fn clob_storage_key(event: &FeedEvent) -> String {
    format!(
        "{}:{}",
        event.source,
        event.asset_id.as_deref().unwrap_or("unknown")
    )
}

/// Extract a persisted top-of-book snapshot from one feed row.
fn persisted_top_of_book(event: &FeedEvent) -> Option<PersistedTopOfBook> {
    Some(PersistedTopOfBook {
        best_bid: event.best_bid?,
        best_ask: event.best_ask?,
        bid_size: event.bid_size?,
        ask_size: event.ask_size?,
    })
}

/// Compare two persisted top-of-book snapshots while ignoring their bucket ids.
fn same_top_of_book(left: &PersistedTopOfBook, right: &PersistedTopOfBook) -> bool {
    left.best_bid.to_bits() == right.best_bid.to_bits()
        && left.best_ask.to_bits() == right.best_ask.to_bits()
        && left.bid_size.to_bits() == right.bid_size.to_bits()
        && left.ask_size.to_bits() == right.ask_size.to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReplayFidelity;

    /// Create one default feed event that tests can refine.
    fn sample_event(source: &str, event_type: &str) -> FeedEvent {
        FeedEvent {
            id: None,
            received_at_ms: 1_000,
            event_at_ms: 1_000,
            received_at_us: Some(1_000_000),
            event_at_us: Some(1_000_000),
            source: source.to_string(),
            event_type: event_type.to_string(),
            source_topic: Some("topic".to_string()),
            source_symbol: Some("BTCUSDT".to_string()),
            connection_id: Some("conn-1".to_string()),
            sequence_key: Some("1".to_string()),
            market_id: Some("mkt-1".to_string()),
            asset_id: Some("asset-1".to_string()),
            price: Some(42_000.0),
            trade_size: None,
            signed_quantity: None,
            best_bid: Some(0.48),
            best_ask: Some(0.52),
            bid_size: Some(100.0),
            ask_size: Some(80.0),
            depth_bid_notional: None,
            depth_ask_notional: None,
            depth_imbalance: None,
            microprice: None,
            payload_json: Some("{\"raw\":true}".to_string()),
            details_json: Some("{\"details\":true}".to_string()),
            fidelity: ReplayFidelity::RawEvent,
        }
    }

    /// Verify that compact trade storage drops payloads and keeps typed fields.
    #[test]
    fn compact_trade_storage_drops_raw_payloads() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::Compact);
        let event = sample_event("binance", "aggTrade");
        let stored = state.prepare_binance_trade(event, 0.25, Some(-0.25));
        assert_eq!(stored.trade_size, Some(0.25));
        assert_eq!(stored.signed_quantity, Some(-0.25));
        assert!(stored.payload_json.is_none());
        assert!(stored.details_json.is_none());
    }

    /// Verify that compact mode skips book-ticker persistence entirely.
    #[test]
    fn compact_mode_skips_book_ticker_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::Compact);
        assert!(
            state
                .prepare_binance_book_ticker(sample_event("binance", "bookTicker"))
                .is_none()
        );
    }

    /// Verify that replay-grade mode persists compact book-ticker rows.
    #[test]
    fn replay_grade_mode_persists_compact_book_ticker_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::ReplayGrade);
        let stored = state
            .prepare_binance_book_ticker(sample_event("binance", "bookTicker"))
            .unwrap();
        assert_eq!(stored.best_bid, Some(0.48));
        assert_eq!(stored.best_ask, Some(0.52));
        assert!(stored.payload_json.is_none());
        assert!(stored.details_json.is_none());
    }

    /// Verify that full-debug mode preserves book-ticker payloads.
    #[test]
    fn full_debug_mode_preserves_book_ticker_payloads() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::FullDebug);
        let stored = state
            .prepare_binance_book_ticker(sample_event("binance", "bookTicker"))
            .unwrap();
        assert!(stored.payload_json.is_some());
        assert!(stored.details_json.is_some());
    }

    /// Verify that compact depth persistence buckets rows by 250ms.
    #[test]
    fn compact_depth_storage_buckets_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::Compact);
        let bid_levels = vec![
            OrderLevel {
                price: 41_999.0,
                size: 0.4,
            },
            OrderLevel {
                price: 41_998.0,
                size: 0.6,
            },
        ];
        let ask_levels = vec![
            OrderLevel {
                price: 42_001.0,
                size: 0.5,
            },
            OrderLevel {
                price: 42_002.0,
                size: 0.7,
            },
        ];
        let first = state
            .prepare_binance_depth(
                sample_event("binance", "depth"),
                Some("BTCUSDT"),
                &bid_levels,
                &ask_levels,
            )
            .unwrap();
        assert!(first.payload_json.is_none());
        assert!(first.depth_bid_notional.unwrap() > 0.0);
        assert!(first.depth_ask_notional.unwrap() > 0.0);
        assert!(first.depth_imbalance.is_some());
        let mut second_event = sample_event("binance", "depth");
        second_event.received_at_ms = 1_120;
        assert!(
            state
                .prepare_binance_depth(second_event, Some("BTCUSDT"), &bid_levels, &ask_levels)
                .is_none()
        );
        let mut third_event = sample_event("binance", "depth");
        third_event.received_at_ms = 1_300;
        assert!(
            state
                .prepare_binance_depth(third_event, Some("BTCUSDT"), &bid_levels, &ask_levels)
                .is_some()
        );
    }

    /// Verify that compact `CLOB` top-of-book rows deduplicate within one bucket.
    #[test]
    fn compact_clob_top_of_book_deduplicates_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::Compact);
        let first = state
            .prepare_clob_top_of_book(sample_event("clob_up", "best_bid_ask"))
            .unwrap();
        assert!(first.microprice.is_some());
        let mut second = sample_event("clob_up", "price_change");
        second.received_at_ms = 1_050;
        assert!(state.prepare_clob_top_of_book(second).is_none());
        let mut third = sample_event("clob_up", "best_bid_ask");
        third.received_at_ms = 1_250;
        assert!(state.prepare_clob_top_of_book(third).is_none());
        let mut fourth = sample_event("clob_up", "best_bid_ask");
        fourth.received_at_ms = 1_300;
        fourth.best_bid = Some(0.49);
        assert!(state.prepare_clob_top_of_book(fourth).is_some());
    }

    /// Verify that size-only CLOB top-of-book changes are preserved.
    #[test]
    fn compact_clob_top_of_book_preserves_size_only_changes() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::ReplayGrade);
        assert!(
            state
                .prepare_clob_top_of_book(sample_event("clob_up", "price_change"))
                .is_some()
        );
        let mut size_change = sample_event("clob_up", "price_change");
        size_change.bid_size = Some(125.0);

        assert!(state.prepare_clob_top_of_book(size_change).is_some());
    }

    /// Verify that compact `CLOB` book snapshots persist once per connection.
    #[test]
    fn compact_clob_book_snapshot_persists_once_per_connection() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::Compact);
        let first = state
            .prepare_clob_book_snapshot(sample_event("clob_up", "book"))
            .unwrap();
        assert!(first.payload_json.is_none());
        assert!(
            state
                .prepare_clob_book_snapshot(sample_event("clob_up", "book"))
                .is_none()
        );
        let mut reconnect = sample_event("clob_up", "book");
        reconnect.connection_id = Some("conn-2".to_string());
        assert!(state.prepare_clob_book_snapshot(reconnect).is_some());
    }

    /// Verify that replay-grade mode skips CLOB last-trade telemetry.
    #[test]
    fn replay_grade_skips_clob_last_trade_price_meta_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::ReplayGrade);
        assert!(
            state
                .prepare_clob_meta(sample_event("clob", "last_trade_price"))
                .is_none()
        );
    }

    /// Verify that full-debug mode preserves CLOB last-trade telemetry.
    #[test]
    fn full_debug_preserves_clob_last_trade_price_meta_rows() {
        let mut state = FeedEventStorageState::new(FeedEventStorageProfile::FullDebug);
        assert!(
            state
                .prepare_clob_meta(sample_event("clob", "last_trade_price"))
                .is_some()
        );
    }
}
