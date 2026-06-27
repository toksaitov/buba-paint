use anyhow::{Context, bail};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::Config;
use crate::types::LiveStrategyReadiness;

/// Compact strategy readiness description shared with the sidecar and the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyReadiness {
    pub strategy: String,
    pub readiness: String,
    pub enabled: bool,
}

/// Budget and risk caps passed to the live sidecar during preflight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveBudgetLimits {
    pub cash_cap_usd: f64,
    pub max_single_order_usd: f64,
    pub max_open_notional_usd: f64,
    pub max_daily_loss_usd: f64,
    pub max_session_drawdown_usd: f64,
    pub min_required_cash_usd: f64,
}

/// One live preflight request sent to the authenticated Polymarket sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LivePreflightRequest {
    pub execution_mode: String,
    pub clob_api_url: String,
    pub gamma_api_url: String,
    pub strategy_readiness: Vec<StrategyReadiness>,
    pub budget_limits: LiveBudgetLimits,
}

/// One live preflight result returned by the sidecar.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveCheckStatus {
    Ok,
    Failed,
}

impl LiveCheckStatus {
    /// Convert one boolean readiness outcome into a serialized status.
    #[must_use]
    pub fn from_bool(ok: bool) -> Self {
        if ok { Self::Ok } else { Self::Failed }
    }
}

/// One live preflight result returned by the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LivePreflightResponse {
    pub ok: bool,
    pub mode: String,
    pub wallet_address: Option<String>,
    pub proxy_wallet: Option<String>,
    #[serde(default)]
    pub signature_type: Option<i64>,
    pub geoblock_status: LiveCheckStatus,
    pub geoblock_country_code: Option<String>,
    #[serde(default)]
    pub geoblock_ip: Option<String>,
    pub auth_status: LiveCheckStatus,
    pub clock_status: LiveCheckStatus,
    pub allowance_status: LiveCheckStatus,
    pub user_stream_status: LiveCheckStatus,
    pub available_cash_usd: Option<f64>,
    pub legal_order_min_usd: Option<f64>,
    pub details_json: Option<String>,
    pub errors: Vec<String>,
}

/// One live account-state snapshot returned by the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveAccountState {
    pub timestamp_ms: u64,
    pub wallet_address: Option<String>,
    pub proxy_wallet: Option<String>,
    pub cash_available: f64,
    pub cash_reserved_for_orders: f64,
    pub inventory_mark_value: f64,
    pub redeemable_value: f64,
    pub pending_redeem_value: f64,
    pub total_equity: f64,
    pub allowance_available: Option<f64>,
    pub details_json: Option<String>,
}

/// One order intent request sent to the sidecar for real venue execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveOrderIntentRequest {
    pub session_id: i64,
    pub intent_id: i64,
    pub market_id: String,
    pub token_id: String,
    pub side: String,
    pub order_type: String,
    pub limit_price: f64,
    pub size: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_usd: Option<f64>,
    pub client_order_id: String,
    pub details_json: Option<String>,
}

/// One sidecar order submission response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveOrderIntentResponse {
    pub ok: bool,
    pub venue_order_id: Option<String>,
    pub client_order_id: String,
    pub status: String,
    pub status_reason: Option<String>,
    pub accepted_size: Option<f64>,
    pub details_json: Option<String>,
}

/// One sidecar order cancellation response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveCancellationResponse {
    pub ok: bool,
    pub cancelled: u64,
    pub details_json: Option<String>,
}

/// One sidecar redemption response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveRedemptionResponse {
    pub ok: bool,
    pub submitted: u64,
    pub details_json: Option<String>,
}

/// One sidecar on-chain CTF position read for a single ERC-1155 token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnchainPositionResponse {
    pub token_id: String,
    pub account: String,
    pub balance_raw: String,
    pub collateral_decimals: u32,
}

/// One raw-safe live activity event returned by the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveActivityEvent {
    pub timestamp_ms: u64,
    pub source: String,
    pub event_type: String,
    pub market_id: Option<String>,
    pub order_id: Option<String>,
    pub trade_id: Option<String>,
    pub asset_id: Option<String>,
    pub side: Option<String>,
    pub price: Option<f64>,
    pub size: Option<f64>,
    pub status: Option<String>,
    pub details_json: Option<String>,
}

/// One sidecar activity snapshot used for live reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveActivityResponse {
    pub timestamp_ms: u64,
    pub user_stream_status: LiveCheckStatus,
    pub user_stream_events: Vec<LiveActivityEvent>,
    pub clob_trades: Vec<LiveActivityEvent>,
    pub details_json: Option<String>,
}

/// Thin `HTTP` client for the local Polymarket live sidecar.
#[derive(Clone)]
pub struct LiveSidecarClient {
    base_url: String,
    client: reqwest::Client,
    request_timeout: Duration,
    emergency_timeout: Duration,
}

impl LiveSidecarClient {
    /// Construct a new client from a configured base URL.
    #[must_use]
    pub fn new(base_url: &str) -> Self {
        Self::with_timeouts(base_url, Duration::from_secs(5), Duration::from_secs(2))
    }

    /// Construct a new client from bot config.
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self::with_timeouts(
            &config.live_sidecar_url,
            Duration::from_millis(config.live_sidecar_request_timeout_ms.max(1)),
            Duration::from_millis(config.live_sidecar_emergency_timeout_ms.max(1)),
        )
    }

    /// Construct a new client with explicit request budgets.
    #[must_use]
    pub fn with_timeouts(
        base_url: &str,
        request_timeout: Duration,
        emergency_timeout: Duration,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            request_timeout,
            emergency_timeout,
        }
    }

    /// Return the current sidecar health payload.
    pub async fn health(&self) -> anyhow::Result<serde_json::Value> {
        self.get_json("/health").await
    }

    /// Execute a live preflight against the authenticated venue boundary.
    pub async fn preflight(&self, config: &Config) -> anyhow::Result<LivePreflightResponse> {
        self.post_json("/preflight", &LivePreflightRequest::from_config(config))
            .await
    }

    /// Fetch the current live account-state decomposition.
    pub async fn account_state(&self) -> anyhow::Result<LiveAccountState> {
        self.get_json("/account").await
    }

    /// Fetch raw-safe venue activity used to reconcile orders and fills.
    pub async fn activity(&self) -> anyhow::Result<LiveActivityResponse> {
        self.get_json("/activity").await
    }

    /// Submit one live order intent to the authenticated venue boundary.
    pub async fn submit_order_intent(
        &self,
        request: &LiveOrderIntentRequest,
    ) -> anyhow::Result<LiveOrderIntentResponse> {
        self.post_json("/orders", request).await
    }

    /// Cancel one live venue order by venue order ID.
    pub async fn cancel_order(&self, order_id: &str) -> anyhow::Result<LiveCancellationResponse> {
        self.post_json(
            "/cancel",
            &serde_json::json!({
                "order_id": order_id,
            }),
        )
        .await
    }

    /// Cancel every currently open live order through the sidecar.
    pub async fn cancel_all(&self) -> anyhow::Result<LiveCancellationResponse> {
        self.post_json_with_timeout(
            "/cancel-all",
            &serde_json::json!({}),
            self.emergency_timeout,
        )
        .await
    }

    /// Trigger redemption for all currently redeemable positions.
    pub async fn redeem_all(&self) -> anyhow::Result<LiveRedemptionResponse> {
        self.post_json("/redeem-all", &serde_json::json!({})).await
    }

    /// Read the on-chain CTF balance for one token, defaulting to the sidecar wallet.
    pub async fn onchain_position(
        &self,
        token_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<OnchainPositionResponse> {
        let mut body = serde_json::Map::new();
        body.insert(
            "token_id".to_string(),
            serde_json::Value::String(token_id.to_string()),
        );
        if let Some(account) = account {
            body.insert(
                "account".to_string(),
                serde_json::Value::String(account.to_string()),
            );
        }
        self.post_json("/onchain-position", &serde_json::Value::Object(body))
            .await
    }

    /// Issue one sidecar `GET` request and decode its JSON response.
    async fn get_json<T>(&self, path: &str) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
            .timeout(self.request_timeout)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        Self::decode_response(response, &url).await
    }

    /// Issue one sidecar `POST` request with a JSON body and decode the response.
    async fn post_json<T, B>(&self, path: &str, body: &B) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .timeout(self.request_timeout)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        Self::decode_response(response, &url).await
    }

    /// Issue one sidecar `POST` with a custom timeout and decode the response.
    async fn post_json_with_timeout<T, B>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .timeout(timeout)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        Self::decode_response(response, &url).await
    }

    /// Decode one sidecar response or raise a detailed transport error.
    async fn decode_response<T>(response: reqwest::Response, url: &str) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status == StatusCode::NOT_IMPLEMENTED {
                bail!("sidecar endpoint {url} is not implemented: {body}");
            }
            bail!("sidecar request to {url} failed with {status}: {body}");
        }
        response
            .json::<T>()
            .await
            .with_context(|| format!("decoding sidecar response from {url}"))
    }
}

impl LivePreflightRequest {
    /// Build one preflight request from the current bot config.
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self {
            execution_mode: config.execution_mode.as_str().to_string(),
            clob_api_url: config.clob_api_url.clone(),
            gamma_api_url: config.gamma_api_url.clone(),
            strategy_readiness: strategy_readiness_matrix(config),
            budget_limits: LiveBudgetLimits {
                cash_cap_usd: config.live_session_cash_cap_usd,
                max_single_order_usd: config.live_max_single_order_usd,
                max_open_notional_usd: config.live_max_open_notional_usd,
                max_daily_loss_usd: config.live_max_daily_loss_usd,
                max_session_drawdown_usd: config.live_max_session_drawdown_usd,
                min_required_cash_usd: config.live_min_required_cash_usd,
            },
        }
    }
}

/// Return the current live-readiness matrix for every strategy family.
#[must_use]
pub fn strategy_readiness_matrix(config: &Config) -> Vec<StrategyReadiness> {
    vec![
        StrategyReadiness {
            strategy: "latency-arb".to_string(),
            readiness: LiveStrategyReadiness::LiveReadyV1.as_str().to_string(),
            enabled: config.latency_arb_enabled,
        },
        StrategyReadiness {
            strategy: "calm-persistence".to_string(),
            readiness: LiveStrategyReadiness::LiveSupportedButDisabled
                .as_str()
                .to_string(),
            enabled: config.calm_persistence_enabled,
        },
        StrategyReadiness {
            strategy: "spread-capture".to_string(),
            readiness: LiveStrategyReadiness::NotLiveV1.as_str().to_string(),
            enabled: config.spread_capture_enabled,
        },
    ]
}

#[cfg(test)]
#[path = "tests/live_sidecar_tests.rs"]
mod tests;
