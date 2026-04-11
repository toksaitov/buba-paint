use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::{Config, ExecutionMode};
use crate::live_sidecar::{LivePreflightRequest, LiveSidecarClient, strategy_readiness_matrix};

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

/// Verifies that sidecar client surfaces not-implemented endpoints explicitly.
#[tokio::test]
async fn live_sidecar_client_reports_not_implemented_endpoints() {
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
