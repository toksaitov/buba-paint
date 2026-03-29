// Execution abstraction layer.
//
// Defines the `Executor` trait and two implementations:
//   - `PaperExecutor`: current paper trading behavior (instant fills via SQLite)
//   - `LiveExecutor`: stub for future real-money trading via Polymarket SDK
//
// The live bot uses `Box<dyn Executor>` so the execution mode can be selected
// at startup via config without changing the trading loop.

use anyhow::Result;

use crate::types::SignalDirection;

/// Describes a filled order (or simulated fill).
#[derive(Debug, Clone)]
pub struct Fill {
    pub trade_id: i64,
    pub side: SignalDirection,
    pub entry_price: f64,
    pub size: f64,
    pub token_id: String,
}

/// Execution backend. Paper trading writes to the database; live trading would
/// place orders on the Polymarket CLOB.
pub trait Executor: Send + Sync {
    /// Place an order and return the fill. For paper trading this is instant.
    /// For live trading this would submit to the CLOB and wait for fill.
    #[allow(clippy::too_many_arguments)]
    fn place_order(
        &self,
        signal: &crate::types::Signal,
        window: &crate::types::MarketWindow,
        is_batch: bool,
        available_liquidity_tokens: f64,
        bankroll: &mut crate::bankroll::BankrollManager,
        config: &crate::config::Config,
        clock: &dyn crate::clock::Clock,
        db: &crate::db::database::Database,
        position_manager: &mut crate::position_manager::PositionManager,
    ) -> Option<Fill>;
}

/// Paper trading executor. Delegates to the position manager for instant fills.
pub struct PaperExecutor;

impl Executor for PaperExecutor {
    fn place_order(
        &self,
        signal: &crate::types::Signal,
        window: &crate::types::MarketWindow,
        is_batch: bool,
        available_liquidity_tokens: f64,
        bankroll: &mut crate::bankroll::BankrollManager,
        config: &crate::config::Config,
        clock: &dyn crate::clock::Clock,
        db: &crate::db::database::Database,
        position_manager: &mut crate::position_manager::PositionManager,
    ) -> Option<Fill> {
        let trade = position_manager.try_open(
            signal,
            window,
            is_batch,
            available_liquidity_tokens,
            db,
            bankroll,
            config,
            clock,
        )?;
        Some(Fill {
            trade_id: trade.id.unwrap_or(-1),
            side: trade.side,
            entry_price: trade.entry_price,
            size: trade.size,
            token_id: trade.token_id,
        })
    }
}

/// Live trading executor stub. Panics if called. Will be implemented when
/// we're ready for real-money trading with the Polymarket SDK.
pub struct LiveExecutor;

impl Executor for LiveExecutor {
    fn place_order(
        &self,
        _signal: &crate::types::Signal,
        _window: &crate::types::MarketWindow,
        _is_batch: bool,
        _available_liquidity_tokens: f64,
        _bankroll: &mut crate::bankroll::BankrollManager,
        _config: &crate::config::Config,
        _clock: &dyn crate::clock::Clock,
        _db: &crate::db::database::Database,
        _position_manager: &mut crate::position_manager::PositionManager,
    ) -> Option<Fill> {
        panic!(
            "LiveExecutor is not implemented. Do NOT use execution_mode=live \
             without implementing real order placement."
        );
    }
}

/// Create the appropriate executor based on the execution mode string.
pub fn create_executor(mode: &str) -> Result<Box<dyn Executor>> {
    match mode {
        "paper" => Ok(Box::new(PaperExecutor)),
        "live" => anyhow::bail!(
            "Live execution mode is not yet implemented. \
             Use execution_mode=paper for paper trading."
        ),
        other => anyhow::bail!("Unknown execution mode: {other}. Use 'paper' or 'live'."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_paper_executor() {
        let executor = create_executor("paper");
        assert!(executor.is_ok());
    }

    #[test]
    fn create_live_executor_fails() {
        let executor = create_executor("live");
        assert!(executor.is_err());
    }

    #[test]
    fn create_unknown_executor_fails() {
        let executor = create_executor("unknown");
        assert!(executor.is_err());
    }

    #[test]
    #[should_panic(expected = "LiveExecutor is not implemented")]
    fn live_executor_panics() {
        use crate::clock::BacktestClock;
        use crate::config::Config;
        use crate::db::database::Database;
        use crate::position_manager::PositionManager;
        use crate::types::{MarketWindow, Signal, SignalDirection};

        let executor = LiveExecutor;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
        let config = Config::default();
        let clock = BacktestClock::new();
        let mut bankroll = crate::bankroll::BankrollManager::new(200.0, &config, &db, &clock);
        let mut pm = PositionManager::new();

        let signal = Signal {
            timestamp: 1000,
            strategy: "test".to_string(),
            direction: SignalDirection::Up,
            confidence: 0.7,
            binance_price: 42000.0,
            chainlink_price: 42000.0,
            up_ask: 0.50,
            down_ask: 0.50,
            up_bid: 0.48,
            down_bid: 0.48,
            metadata: serde_json::json!({}),
        };
        let window = MarketWindow {
            market_id: "m1".to_string(),
            question: "test".to_string(),
            up_token_id: "u".to_string(),
            down_token_id: "d".to_string(),
            condition_id: "c".to_string(),
            start_time: 0,
            end_time: 300000,
            slug: "test".to_string(),
        };

        executor.place_order(
            &signal,
            &window,
            false,
            f64::MAX,
            &mut bankroll,
            &config,
            &clock,
            &db,
            &mut pm,
        );
    }
}
