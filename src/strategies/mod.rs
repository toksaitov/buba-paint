use crate::types::Signal;

pub mod latency_arb;
pub mod spread_capture;

pub enum StrategyResult {
    None,
    Single(Signal),
    Batch(Vec<Signal>),
}

pub trait Strategy: Send {
    fn name(&self) -> &'static str;
    fn evaluate(
        &mut self,
        ctx: &crate::types::StrategyContext,
        config: &crate::config::Config,
        now: u64,
    ) -> StrategyResult;
}
