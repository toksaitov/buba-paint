use std::collections::HashMap;

use crate::types::{
    StrategyRejection, StrategyRejectionReason, StrategyRejectionSample,
    StrategyRejectionSummaryRecord,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StrategyRejectionKey {
    market_id: String,
    strategy: String,
    reason: StrategyRejectionReason,
}

#[derive(Debug, Clone, Default)]
struct NumericAggregate {
    sum: f64,
    count: u64,
}

impl NumericAggregate {
    /// Records one optional numeric sample into the running sum.
    fn record(&mut self, value: Option<f64>) {
        if let Some(value) = value {
            self.sum += value;
            self.count += 1;
        }
    }

    /// Returns the arithmetic mean when at least one value was recorded.
    fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        Some(self.sum / self.count as f64)
    }
}

#[derive(Debug, Clone, Default)]
struct StrategyRejectionAccumulator {
    count: u64,
    last: StrategyRejectionSample,
    up_ask: NumericAggregate,
    down_ask: NumericAggregate,
    total_ask: NumericAggregate,
    external_bias: NumericAggregate,
    up_edge: NumericAggregate,
    down_edge: NumericAggregate,
    expected_fee: NumericAggregate,
    expected_slippage: NumericAggregate,
    expected_edge: NumericAggregate,
    quote_age_ms: NumericAggregate,
    book_staleness_ms: NumericAggregate,
    window_time_remaining_ms: NumericAggregate,
    inter_leg_skew_ms: NumericAggregate,
    available_spread_budget: NumericAggregate,
    required_pair_notional: NumericAggregate,
    required_pair_units: NumericAggregate,
    quote_churn_per_s: NumericAggregate,
    move_velocity: NumericAggregate,
}

impl StrategyRejectionAccumulator {
    /// Records one rejected strategy evaluation.
    fn record(&mut self, sample: &StrategyRejectionSample) {
        self.count += 1;
        self.last = sample.clone();
        self.up_ask.record(sample.up_ask);
        self.down_ask.record(sample.down_ask);
        self.total_ask.record(sample.total_ask);
        self.external_bias.record(sample.external_bias);
        self.up_edge.record(sample.up_edge);
        self.down_edge.record(sample.down_edge);
        self.expected_fee.record(sample.expected_fee);
        self.expected_slippage.record(sample.expected_slippage);
        self.expected_edge.record(sample.expected_edge);
        self.quote_age_ms
            .record(sample.quote_age_ms.map(|value| value as f64));
        self.book_staleness_ms
            .record(sample.book_staleness_ms.map(|value| value as f64));
        self.available_spread_budget
            .record(sample.available_spread_budget);
        self.required_pair_notional
            .record(sample.required_pair_notional);
        self.required_pair_units.record(sample.required_pair_units);
        self.window_time_remaining_ms
            .record(sample.window_time_remaining_ms.map(|value| value as f64));
        self.inter_leg_skew_ms
            .record(sample.inter_leg_skew_ms.map(|value| value as f64));
        self.quote_churn_per_s.record(sample.quote_churn_per_s);
        self.move_velocity.record(sample.move_velocity);
    }

    /// Builds the mean sample used in persisted rejection summaries.
    fn mean_sample(&self) -> StrategyRejectionSample {
        StrategyRejectionSample {
            up_ask: self.up_ask.mean(),
            down_ask: self.down_ask.mean(),
            total_ask: self.total_ask.mean(),
            external_bias: self.external_bias.mean(),
            up_edge: self.up_edge.mean(),
            down_edge: self.down_edge.mean(),
            expected_fee: self.expected_fee.mean(),
            expected_slippage: self.expected_slippage.mean(),
            expected_edge: self.expected_edge.mean(),
            quote_age_ms: self.quote_age_ms.mean().map(|value| value.round() as u64),
            book_staleness_ms: self
                .book_staleness_ms
                .mean()
                .map(|value| value.round() as u64),
            available_spread_budget: self.available_spread_budget.mean(),
            required_pair_notional: self.required_pair_notional.mean(),
            required_pair_units: self.required_pair_units.mean(),
            window_time_remaining_ms: self
                .window_time_remaining_ms
                .mean()
                .map(|value| value.round() as u64),
            inter_leg_skew_ms: self
                .inter_leg_skew_ms
                .mean()
                .map(|value| value.round() as u64),
            quote_churn_per_s: self.quote_churn_per_s.mean(),
            move_velocity: self.move_velocity.mean(),
        }
    }
}

#[derive(Debug, Default)]
pub struct StrategyRejectionTracker {
    aggregates: HashMap<StrategyRejectionKey, StrategyRejectionAccumulator>,
}

impl StrategyRejectionTracker {
    /// Creates an empty in-memory tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one rejected strategy evaluation for the active market.
    pub fn record(&mut self, market_id: &str, rejection: &StrategyRejection) {
        let key = StrategyRejectionKey {
            market_id: market_id.to_string(),
            strategy: rejection.strategy.clone(),
            reason: rejection.reason,
        };
        self.aggregates
            .entry(key)
            .or_default()
            .record(&rejection.sample);
    }

    /// Drains all accumulated rejection summaries into persisted records.
    pub fn drain_all(&mut self, timestamp_ms: u64) -> Vec<StrategyRejectionSummaryRecord> {
        let drained = std::mem::take(&mut self.aggregates);
        drained
            .into_iter()
            .map(|(key, aggregate)| build_summary_record(timestamp_ms, key, &aggregate))
            .collect()
    }

    /// Builds a non-destructive snapshot of every current rejection summary.
    #[must_use]
    pub fn snapshot_all(&self, timestamp_ms: u64) -> Vec<StrategyRejectionSummaryRecord> {
        self.aggregates
            .iter()
            .map(|(key, aggregate)| build_summary_record(timestamp_ms, key.clone(), aggregate))
            .collect()
    }

    /// Drains one market's accumulated rejection summaries into persisted records.
    pub fn drain_market(
        &mut self,
        market_id: &str,
        timestamp_ms: u64,
    ) -> Vec<StrategyRejectionSummaryRecord> {
        let mut drained = Vec::new();
        self.aggregates.retain(|key, aggregate| {
            if key.market_id == market_id {
                drained.push(build_summary_record(timestamp_ms, key.clone(), aggregate));
                false
            } else {
                true
            }
        });
        drained
    }
}

/// Converts one in-memory aggregate into the persisted summary row.
fn build_summary_record(
    timestamp_ms: u64,
    key: StrategyRejectionKey,
    aggregate: &StrategyRejectionAccumulator,
) -> StrategyRejectionSummaryRecord {
    let details_json = serde_json::json!({
        "last": aggregate.last.to_json(),
        "mean": aggregate.mean_sample().to_json(),
    });
    StrategyRejectionSummaryRecord {
        timestamp_ms,
        market_id: key.market_id,
        strategy: key.strategy,
        reason: key.reason.to_string(),
        count: aggregate.count,
        details_json: details_json.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that repeated rejections aggregate counts and running means.
    #[test]
    fn tracker_aggregates_rejections() {
        let mut tracker = StrategyRejectionTracker::new();
        tracker.record(
            "mkt-1",
            &StrategyRejection::new(
                "latency-arb",
                StrategyRejectionReason::DirectionNotSelected,
                StrategyRejectionSample {
                    up_ask: Some(0.40),
                    external_bias: Some(1.2),
                    ..StrategyRejectionSample::default()
                },
            ),
        );
        tracker.record(
            "mkt-1",
            &StrategyRejection::new(
                "latency-arb",
                StrategyRejectionReason::DirectionNotSelected,
                StrategyRejectionSample {
                    up_ask: Some(0.60),
                    external_bias: Some(1.4),
                    ..StrategyRejectionSample::default()
                },
            ),
        );

        let rows = tracker.drain_market("mkt-1", 1234);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 2);
        let details: serde_json::Value = serde_json::from_str(&rows[0].details_json).unwrap();
        assert_eq!(details["last"]["upAsk"], 0.6);
        assert_eq!(details["mean"]["upAsk"], 0.5);
        let mean_external_bias = details["mean"]["externalBias"].as_f64().unwrap();
        assert!((mean_external_bias - 1.3).abs() < 1e-9);
    }
}
