// Read-only Polymarket CLOB client wrapper.
//
// Uses the official polymarket-client-sdk to query market data and resolution
// status. This module deliberately exposes NO trading methods. The SDK client
// is created without a private key, making order placement impossible.

use crate::types::SignalDirection;

const CLOB_HOST: &str = "https://clob.polymarket.com";

/// Read-only wrapper around the Polymarket CLOB client.
///
/// Provides typed access to market resolution status without any ability to
/// place orders or modify on-chain state.
pub struct PolymarketClient {
    client: polymarket_client_sdk::clob::Client,
}

impl PolymarketClient {
    /// Create a read-only client. No private key, no trading capability.
    pub fn new_read_only() -> anyhow::Result<Self> {
        let client = polymarket_client_sdk::clob::Client::new(
            CLOB_HOST,
            polymarket_client_sdk::clob::Config::default(),
        )?;
        Ok(Self { client })
    }

    /// Check if a market is resolved and return the winning direction.
    ///
    /// Queries the CLOB API for the market identified by `condition_id` and
    /// looks for a token with `winner: true`. Returns `None` if the market is
    /// not yet resolved, the API is unreachable, or the response is unexpected.
    pub async fn get_resolution(&self, condition_id: &str) -> Option<SignalDirection> {
        let market = self.client.market(condition_id).await.ok()?;
        for token in &market.tokens {
            if token.winner {
                let outcome = token.outcome.to_lowercase();
                return match outcome.as_str() {
                    "up" | "yes" => Some(SignalDirection::Up),
                    "down" | "no" => Some(SignalDirection::Down),
                    _ => None,
                };
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_read_only_creates_client() {
        let client = PolymarketClient::new_read_only();
        assert!(client.is_ok());
    }
}
