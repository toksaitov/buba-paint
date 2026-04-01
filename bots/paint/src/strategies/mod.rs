use crate::types::Signal;

pub mod latency_arb;
pub mod spread_capture;

pub enum StrategyResult {
    None,
    Single(Box<Signal>),
    Batch(Vec<Signal>),
}

pub trait Strategy: Send {
    /// Returns the stable strategy name used in logs and persisted records.
    fn name(&self) -> &'static str;

    /// Evaluates the current market context and returns any generated signals.
    fn evaluate(
        &mut self,
        ctx: &crate::types::StrategyContext,
        config: &crate::config::Config,
        now: u64,
    ) -> StrategyResult;
}
