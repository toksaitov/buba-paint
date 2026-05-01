use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::{Config, ExecutionMode};
use crate::live_sidecar::{
    LiveCheckStatus, LiveOrderIntentRequest, LivePreflightRequest, LiveSidecarClient,
    strategy_readiness_matrix,
};

/// Verifies that readiness matrix reflects the configured strategy flags.
#[test]
fn strategy_readiness_matrix_reflects_live_rollout_policy() {
    let mut config = Config::default();
    config.calm_persistence_enabled = true;

    let matrix = strategy_readiness_matrix(&config);
    assert_eq!(matrix.len(), 3);
    assert_eq!(matrix[0].strategy, "latency-arb");
    assert_eq!(matrix[0].readiness, "live_ready_v1");
    assert!(matrix[0].enabled);
    assert_eq!(matrix[1].strategy, "calm-persistence");
    assert_eq!(matrix[1].readiness, "live_supported_but_disabled");
    assert!(matrix[1].enabled);
    assert_eq!(matrix[2].strategy, "spread-capture");
    assert_eq!(matrix[2].readiness, "not_live_v1");
    assert!(matrix[2].enabled);
}

/// Verifies that preflight requests inherit live budgeting and venue URLs from config.
#[test]
fn preflight_request_uses_config_budget_limits() {
    let mut config = Config::default();
    config.execution_mode = ExecutionMode::LiveReadonly;
    config.live_session_cash_cap_usd = 90.0;
    config.live_max_single_order_usd = 7.5;
    config.live_min_required_cash_usd = 30.0;

    let request = LivePreflightRequest::from_config(&config);
    assert_eq!(request.execution_mode, "live_readonly");
    assert_eq!(request.budget_limits.cash_cap_usd, 90.0);
    assert_eq!(request.budget_limits.max_single_order_usd, 7.5);
    assert_eq!(request.budget_limits.min_required_cash_usd, 30.0);
    assert_eq!(request.clob_api_url, config.clob_api_url);
}

/// Verifies that the sidecar client decodes a successful preflight response.
#[tokio::test]
async fn live_sidecar_client_preflight_round_trip() {
    let server = MockServer::start().await;
    let mut config = Config::default();
    config.execution_mode = ExecutionMode::LiveReadonly;
    config.live_sidecar_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/preflight"))
        .and(body_json(
            serde_json::to_value(LivePreflightRequest::from_config(&config)).unwrap(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "mode": "live_readonly",
            "wallet_address": "0xwallet",
            "proxy_wallet": "0xproxy",
            "geoblock_status": "ok",
            "geoblock_country_code": "IE",
            "auth_status": "ok",
            "clock_status": "ok",
            "allowance_status": "ok",
            "user_stream_status": "ok",
            "available_cash_usd": 88.0,
            "legal_order_min_usd": 5.0,
            "details_json": null,
            "errors": []
        })))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let response = client.preflight(&config).await.unwrap();
    assert!(response.ok);
    assert_eq!(response.proxy_wallet.as_deref(), Some("0xproxy"));
    assert_eq!(response.available_cash_usd, Some(88.0));
}

/// Verifies that sidecar client surfaces write endpoint transport failures.
#[tokio::test]
async fn live_sidecar_client_reports_write_endpoint_transport_failures() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/redeem-all"))
        .respond_with(ResponseTemplate::new(501).set_body_string("stub provider"))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let error = client.redeem_all().await.unwrap_err().to_string();
    assert!(error.contains("not implemented"));
    assert!(error.contains("/redeem-all"));
}

/// Verifies that live order intents serialize the CLOB V2 market-order dollar amount.
#[tokio::test]
async fn live_sidecar_client_serializes_amount_usd_for_buy_orders() {
    let server = MockServer::start().await;
    let request = LiveOrderIntentRequest {
        session_id: 1,
        intent_id: 2,
        market_id: "0xcondition".to_string(),
        token_id: "up-token".to_string(),
        side: "BUY".to_string(),
        order_type: "FOK".to_string(),
        limit_price: 0.5,
        size: 5.0,
        amount_usd: Some(5.0),
        client_order_id: "client-1".to_string(),
        details_json: None,
    };

    Mock::given(method("POST"))
        .and(path("/orders"))
        .and(body_json(serde_json::json!({
            "session_id": 1,
            "intent_id": 2,
            "market_id": "0xcondition",
            "token_id": "up-token",
            "side": "BUY",
            "order_type": "FOK",
            "limit_price": 0.5,
            "size": 5.0,
            "amount_usd": 5.0,
            "client_order_id": "client-1",
            "details_json": null
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "venue_order_id": "0xvenue",
            "client_order_id": "client-1",
            "status": "matched",
            "status_reason": null,
            "accepted_size": 5.0,
            "details_json": "{\"provider\":\"test\"}"
        })))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let response = client.submit_order_intent(&request).await.unwrap();
    assert!(response.ok);
    assert_eq!(response.client_order_id, "client-1");
}

/// Verifies that sidecar client preserves generic transport failures.
#[tokio::test]
async fn live_sidecar_client_reports_transport_failures() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let error = client.account_state().await.unwrap_err().to_string();
    assert!(error.contains("failed with 500"));
    assert!(error.contains("boom"));
}

/// Verifies that additive sidecar health fields remain readable through the Rust client.
#[tokio::test]
async fn live_sidecar_client_health_reports_additive_readiness_fields() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "ready": false,
            "readiness_status": "degraded",
            "mode": "local-sidecar",
            "provider": "polymarket",
            "signature_type": 1,
            "wallet_address": "0xwallet",
            "proxy_wallet": "0xproxy",
            "auth_configured": true,
            "relayer_api_key_present": false,
            "user_stream_status": "failed",
            "last_user_stream_connected_at_ms": 1,
            "last_user_stream_event_at_ms": null,
            "last_user_stream_error": "stream down",
            "last_successful_account_refresh_at_ms": 2,
            "last_account_refresh_error": "positions: timed out",
            "last_user_stream_disconnect_at_ms": 3,
            "last_user_stream_disconnect_reason": "closed",
            "consecutive_user_stream_failures": 4
        })))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let response = client.health().await.unwrap();
    assert_eq!(response["readiness_status"], "degraded");
    assert_eq!(
        response["last_account_refresh_error"],
        "positions: timed out"
    );
    assert_eq!(response["consecutive_user_stream_failures"], 4);
}

/// Verifies that sidecar activity snapshots decode recovered order and trade truth.
#[tokio::test]
async fn live_sidecar_client_activity_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/activity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "timestamp_ms": 1_700_000_000_000_u64,
            "user_stream_status": "ok",
            "user_stream_events": [{
                "timestamp_ms": 1_700_000_000_001_u64,
                "source": "user_stream",
                "event_type": "order",
                "market_id": "mkt-1",
                "order_id": "venue-1",
                "trade_id": null,
                "asset_id": "tok-up",
                "side": "BUY",
                "price": 0.51,
                "size": 5.0,
                "status": "matched",
                "details_json": "{\"source\":\"test\"}"
            }],
            "clob_trades": [{
                "timestamp_ms": 1_700_000_000_002_u64,
                "source": "clob_trades",
                "event_type": "trade",
                "market_id": "mkt-1",
                "order_id": "venue-1",
                "trade_id": "trade-1",
                "asset_id": "tok-up",
                "side": "BUY",
                "price": 0.51,
                "size": 5.0,
                "status": "confirmed",
                "details_json": null
            }],
            "details_json": "{\"provider\":\"test\"}"
        })))
        .mount(&server)
        .await;

    let client = LiveSidecarClient::new(&server.uri());
    let response = client.activity().await.unwrap();
    assert_eq!(response.user_stream_status, LiveCheckStatus::Ok);
    assert_eq!(response.user_stream_events.len(), 1);
    assert_eq!(response.clob_trades.len(), 1);
    assert_eq!(response.clob_trades[0].trade_id.as_deref(), Some("trade-1"));
}
