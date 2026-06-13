use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::error::DashboardError;
use crate::research_backend::ResearchWorkBackend;
use crate::research_controller_client::ResearchControllerClient;

/// Verifies a 2xx text/html body returns a descriptive Proxy error, not a serde decode error.
#[tokio::test]
async fn send_rejects_non_json_2xx_with_descriptive_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/research/workers/machines/research"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<!doctype html><html><body>app shell</body></html>",
            "text/html",
        ))
        .mount(&server)
        .await;

    let client = ResearchControllerClient::new(&server.uri(), "secret").unwrap();
    let error = client.get_research_machine("research").await.unwrap_err();

    match error {
        DashboardError::Proxy(message) => {
            assert!(
                message.contains("text/html"),
                "error must name the unexpected content type, got: {message}"
            );
            assert!(
                message.contains("non-JSON"),
                "error must classify the failure, got: {message}"
            );
            assert!(
                !message.contains("expected value at line 1 column 1"),
                "error must not surface a raw serde decode message, got: {message}"
            );
        }
        other => panic!("expected DashboardError::Proxy, got: {other:?}"),
    }
}

/// Verifies a 2xx application/json body still decodes successfully after the guard.
#[tokio::test]
async fn send_accepts_json_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/research/workers/machines/research"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "research",
            "name": "Research",
            "role": "research",
            "ssh_alias": null,
            "status": "idle",
            "details_json": null,
            "created_at": 0,
            "updated_at": 0
        })))
        .mount(&server)
        .await;

    let client = ResearchControllerClient::new(&server.uri(), "secret").unwrap();
    let machine = client.get_research_machine("research").await.unwrap();

    assert_eq!(machine.unwrap().id, "research");
}
