use crate::portfolio::StrategyFamily;
use crate::types::{Signal, StrategyRejection};

pub mod calm_persistence;
pub mod latency_arb;
pub mod spread_capture;

#[derive(Debug)]
pub enum StrategyResult {
    None,
    Rejected(Box<StrategyRejection>),
    Single(Box<Signal>),
    Batch(Vec<Signal>),
}

pub trait Strategy: Send {
    /// Returns the stable strategy name used in logs and persisted records.
    fn name(&self) -> &'static str;

    /// Returns the stable portfolio family used by routing and sleeves.
    fn family(&self) -> StrategyFamily;

    /// Evaluates the current market context and returns any generated signals.
    fn evaluate(
        &mut self,
        ctx: &crate::types::StrategyContext,
        config: &crate::config::Config,
        now: u64,
    ) -> StrategyResult;
}
