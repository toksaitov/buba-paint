use crate::config::AgentConfig;
use crate::error::DashboardError;

/// Proxy an HTTP GET request to an agent.
pub async fn proxy_get(
    agent: &AgentConfig,
    path: &str,
    query: Option<&str>,
) -> Result<serde_json::Value, DashboardError> {
    let mut url = format!("{}{path}", agent.url);
    if let Some(q) = query {
        if !q.is_empty() {
            url.push('?');
            url.push_str(q);
        }
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", agent.secret))
        .send()
        .await
        .map_err(|e| DashboardError::Proxy(format!("request to agent failed: {e}")))?;

    if !resp.status().is_success() {
        let status_code = resp.status().as_u16();
        let msg = extract_agent_error(resp).await;
        return Err(DashboardError::AgentError(status_code, msg));
    }

    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| DashboardError::Proxy(format!("failed to parse agent response: {e}")))
}

/// Proxy an HTTP POST request to an agent.
pub async fn proxy_post(
    agent: &AgentConfig,
    path: &str,
) -> Result<serde_json::Value, DashboardError> {
    let url = format!("{}{path}", agent.url);

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", agent.secret))
        .send()
        .await
        .map_err(|e| DashboardError::Proxy(format!("request to agent failed: {e}")))?;

    if !resp.status().is_success() {
        let status_code = resp.status().as_u16();
        let msg = extract_agent_error(resp).await;
        return Err(DashboardError::AgentError(status_code, msg));
    }

    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| DashboardError::Proxy(format!("failed to parse agent response: {e}")))
}

/// Extract a clean error message from an agent error response.
/// Tries to parse `{"error": "..."}` JSON; falls back to raw body text.
pub(crate) async fn extract_agent_error(resp: reqwest::Response) -> String {
    let body = resp.text().await.unwrap_or_default();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = json.get("error").and_then(|v| v.as_str()) {
            return msg.to_string();
        }
    }
    body
}

#[cfg(test)]
#[path = "tests/proxy_tests.rs"]
mod tests;
