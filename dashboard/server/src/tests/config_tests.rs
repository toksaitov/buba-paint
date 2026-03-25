use super::*;

#[test]
fn parse_toml_config() {
    let toml = r#"
[server]
port = 3001
jwt_secret = "test-secret"

[[agents]]
id = "paint-prod"
name = "BTC Paint (Production)"
url = "http://agent:9090"
secret = "agent-secret"
"#;

    let config: DashboardConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.server.port, 3001);
    assert_eq!(config.server.jwt_secret, "test-secret");
    assert_eq!(config.agents.len(), 1);
    assert_eq!(config.agents[0].id, "paint-prod");
    assert_eq!(config.agents[0].secret, "agent-secret");
}

#[test]
fn parse_config_default_port() {
    let toml = r#"
[server]
jwt_secret = "secret"
"#;

    let config: DashboardConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.server.port, 3001);
}

#[test]
fn parse_config_no_agents() {
    let toml = r#"
[server]
jwt_secret = "secret"
"#;

    let config: DashboardConfig = toml::from_str(toml).unwrap();
    assert!(config.agents.is_empty());
}

#[test]
fn parse_config_multiple_agents() {
    let toml = r#"
[server]
jwt_secret = "secret"

[[agents]]
id = "bot-1"
name = "Bot One"
url = "http://host1:9090"
secret = "s1"

[[agents]]
id = "bot-2"
name = "Bot Two"
url = "http://host2:9090"
secret = "s2"
"#;

    let config: DashboardConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.agents.len(), 2);
    assert_eq!(config.agents[1].id, "bot-2");
}

// -- from_file --

#[test]
fn from_file_reads_toml_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.toml");
    std::fs::write(
        &path,
        r#"
[server]
port = 4000
jwt_secret = "disk-secret"
"#,
    )
    .unwrap();

    let config = DashboardConfig::from_file(path.to_str().unwrap()).unwrap();
    assert_eq!(config.server.port, 4000);
    assert_eq!(config.server.jwt_secret, "disk-secret");
}

#[test]
fn from_file_missing_file_returns_error() {
    let result = DashboardConfig::from_file("/nonexistent/path/config.toml");
    assert!(result.is_err());
}

#[test]
fn from_file_invalid_toml_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is not valid toml {{{").unwrap();

    let result = DashboardConfig::from_file(path.to_str().unwrap());
    assert!(result.is_err());
}
