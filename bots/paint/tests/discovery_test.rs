use std::time::Duration;

use buba_paint::config::Config;
use buba_paint::market_discovery::MarketDiscoveryEvent;
use tokio::time::timeout;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper: build a Config pointing at the given mock server with a fast poll interval.
fn discovery_config(mock_url: &str) -> Config {
    Config {
        gamma_api_url: mock_url.to_string(),
        clob_api_url: mock_url.to_string(),
        gamma_poll_interval: 200,
        ..Config::default()
    }
}

/// Helper: build a Gamma API response body for a valid market window.
///
/// `end_date` should be an RFC 3339 timestamp.  The market is accepted only
/// when the end date is in the future.
fn gamma_response(end_date: &str) -> serde_json::Value {
    serde_json::json!({
        "slug": "btc-updown-5m-test",
        "markets": [{
            "id": "mkt-test-1",
            "question": "Will BTC go up?",
            "conditionId": "cond-test-1",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up-test", "tok-down-test"],
            "endDate": end_date
        }]
    })
}

/// Mock the token fee-rate endpoint used during market discovery enrichment.
async fn mock_fee_rates(mock_server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/fee-rate\?token_id=.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "base_fee": 1000
        })))
        .mount(mock_server)
        .await;
}

/// Verifies that discovery finds active market.
#[tokio::test]
async fn discovery_finds_active_market() {
    let mock_server = MockServer::start().await;

    let future_end = chrono::Utc::now() + chrono::Duration::minutes(5);
    let end_date_str = future_end.to_rfc3339();
    let body = gamma_response(&end_date_str);

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;
    mock_fee_rates(&mock_server).await;

    let cfg = discovery_config(&mock_server.uri());
    let mut handle = buba_paint::market_discovery::run_market_discovery(&cfg).await;

    let event = timeout(Duration::from_secs(10), handle.window_rx.recv())
        .await
        .expect("timed out waiting for NewWindow")
        .expect("channel closed");

    match event {
        MarketDiscoveryEvent::NewWindow(window) => {
            assert_eq!(window.market_id, "mkt-test-1");
            assert_eq!(window.up_token_id, "tok-up-test");
            assert_eq!(window.down_token_id, "tok-down-test");
        }
        other @ MarketDiscoveryEvent::WindowClosed(_) => {
            panic!("expected NewWindow, got {other:?}")
        }
    }
}

/// Verifies that discovery handles 404.
#[tokio::test]
async fn discovery_handles_404() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let cfg = discovery_config(&mock_server.uri());
    let mut handle = buba_paint::market_discovery::run_market_discovery(&cfg).await;

    let result = timeout(Duration::from_secs(2), handle.window_rx.recv()).await;
    assert!(
        result.is_err(),
        "expected timeout (no event on 404), but got an event"
    );
}

/// Verifies that discovery emits window closed.
#[tokio::test]
async fn discovery_emits_window_closed() {
    let mock_server = MockServer::start().await;

    let close_at = chrono::Utc::now() + chrono::Duration::seconds(2);
    let end_date_str = close_at.to_rfc3339();
    let body = gamma_response(&end_date_str);

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;
    mock_fee_rates(&mock_server).await;

    let cfg = discovery_config(&mock_server.uri());
    let mut handle = buba_paint::market_discovery::run_market_discovery(&cfg).await;

    let event1 = timeout(Duration::from_secs(10), handle.window_rx.recv())
        .await
        .expect("timed out waiting for NewWindow")
        .expect("channel closed");
    assert!(
        matches!(event1, MarketDiscoveryEvent::NewWindow(_)),
        "expected NewWindow, got {event1:?}"
    );

    let closed_found = timeout(Duration::from_secs(10), async {
        loop {
            match handle.window_rx.recv().await {
                Some(MarketDiscoveryEvent::WindowClosed(w)) => return w,
                Some(MarketDiscoveryEvent::NewWindow(_)) => {}
                None => panic!("channel closed before WindowClosed"),
            }
        }
    })
    .await
    .expect("timed out waiting for WindowClosed");

    assert_eq!(closed_found.market_id, "mkt-test-1");
}

/// Verifies that discovery recovers from 404 to 200.
#[tokio::test]
async fn discovery_recovers_from_404_to_200() {
    let mock_server = MockServer::start().await;

    let future_end = chrono::Utc::now() + chrono::Duration::minutes(5);
    let end_date_str = future_end.to_rfc3339();
    let body = gamma_response(&end_date_str);

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(404))
        .up_to_n_times(3)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;
    mock_fee_rates(&mock_server).await;

    let cfg = discovery_config(&mock_server.uri());
    let mut handle = buba_paint::market_discovery::run_market_discovery(&cfg).await;

    let event = timeout(Duration::from_secs(15), handle.window_rx.recv())
        .await
        .expect("timed out waiting for NewWindow after 404 recovery")
        .expect("channel closed");

    match event {
        MarketDiscoveryEvent::NewWindow(window) => {
            assert_eq!(window.market_id, "mkt-test-1");
            assert_eq!(window.up_token_id, "tok-up-test");
            assert_eq!(window.down_token_id, "tok-down-test");
        }
        other @ MarketDiscoveryEvent::WindowClosed(_) => {
            panic!("expected NewWindow, got {other:?}")
        }
    }
}
