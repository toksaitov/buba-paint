use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::AgentConfig;
use crate::error::DashboardError;
use crate::proxy;

fn test_agent(url: &str) -> AgentConfig {
    AgentConfig {
        id: "test".into(),
        name: "Test Bot".into(),
        url: url.into(),
        secret: "s".into(),
    }
}

#[tokio::test]
async fn proxy_get_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"balance": 100.0})),
        )
        .mount(&server)
        .await;

    let agent = test_agent(&server.uri());
    let result = proxy::proxy_get(&agent, "/api/status", None).await.unwrap();
    assert_eq!(result["balance"], 100.0);
}

#[tokio::test]
async fn proxy_post_forwards_409() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/start"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "process management disabled (monitor-only mode)"
        })))
        .mount(&server)
        .await;

    let agent = test_agent(&server.uri());
    let err = proxy::proxy_post(&agent, "/api/bot/start")
        .await
        .unwrap_err();
    match err {
        DashboardError::AgentError(409, msg) => {
            assert!(msg.contains("monitor-only"));
        }
        other => panic!("expected AgentError(409, _), got: {other:?}"),
    }
}

#[tokio::test]
async fn proxy_post_forwards_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/start"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": "internal failure"
        })))
        .mount(&server)
        .await;

    let agent = test_agent(&server.uri());
    let err = proxy::proxy_post(&agent, "/api/bot/start")
        .await
        .unwrap_err();
    match err {
        DashboardError::AgentError(500, msg) => {
            assert_eq!(msg, "internal failure");
        }
        other => panic!("expected AgentError(500, _), got: {other:?}"),
    }
}

#[tokio::test]
async fn proxy_unreachable_returns_proxy_error() {
    let agent = test_agent("http://127.0.0.1:1");
    let err = proxy::proxy_get(&agent, "/api/status", None)
        .await
        .unwrap_err();
    assert!(matches!(err, DashboardError::Proxy(_)));
}

#[tokio::test]
async fn proxy_extracts_error_message_from_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(serde_json::json!({"error": "invalid parameters"})),
        )
        .mount(&server)
        .await;

    let agent = test_agent(&server.uri());
    let err = proxy::proxy_get(&agent, "/api/status", None)
        .await
        .unwrap_err();
    match err {
        DashboardError::AgentError(422, msg) => {
            assert_eq!(msg, "invalid parameters");
        }
        other => panic!("expected AgentError(422, _), got: {other:?}"),
    }
}

#[tokio::test]
async fn proxy_falls_back_to_raw_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(500).set_body_string("plain text error"))
        .mount(&server)
        .await;

    let agent = test_agent(&server.uri());
    let err = proxy::proxy_get(&agent, "/api/status", None)
        .await
        .unwrap_err();
    match err {
        DashboardError::AgentError(500, msg) => {
            assert_eq!(msg, "plain text error");
        }
        other => panic!("expected AgentError(500, _), got: {other:?}"),
    }
}
