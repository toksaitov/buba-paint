use std::time::Duration;

use super::*;
use crate::error::AgentError;

// ---------------------------------------------------------------------------
// NoopProcessManager tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn noop_start_returns_error() {
    let mgr = NoopProcessManager::new(None);
    let err = mgr.start().await.unwrap_err();
    assert!(err.to_string().contains("monitor-only"));
    assert!(matches!(err, AgentError::BotControlUnavailable(_)));
}

#[tokio::test]
async fn noop_stop_returns_error() {
    let mgr = NoopProcessManager::new(None);
    let err = mgr.stop().await.unwrap_err();
    assert!(err.to_string().contains("monitor-only"));
    assert!(matches!(err, AgentError::BotControlUnavailable(_)));
}

#[tokio::test]
async fn noop_restart_returns_error() {
    let mgr = NoopProcessManager::new(None);
    let err = mgr.restart().await.unwrap_err();
    assert!(err.to_string().contains("monitor-only"));
    assert!(matches!(err, AgentError::BotControlUnavailable(_)));
}

#[tokio::test]
async fn noop_status_returns_inactive() {
    let mgr = NoopProcessManager::new(None);
    let status = mgr.status().await.unwrap();
    assert!(!status.active);
    assert!(status.pid.is_none());
    assert!(status.uptime_secs.is_none());
}

#[tokio::test]
async fn noop_logs_no_path() {
    let mgr = NoopProcessManager::new(None);
    let lines = mgr.logs(100).await.unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("monitor-only"));
}

#[tokio::test]
async fn noop_logs_with_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.log");
    std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

    let mgr = NoopProcessManager::new(Some(path.to_str().unwrap().to_string()));
    let lines = mgr.logs(2).await.unwrap();
    assert_eq!(lines, vec!["line2", "line3"]);
}

// ---------------------------------------------------------------------------
// ChildProcessManager tests
// ---------------------------------------------------------------------------

fn sleep_config(secs: u32) -> ProcessConfig {
    ProcessConfig {
        command: vec!["sleep".to_string(), secs.to_string()],
        max_restarts: 0,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    }
}

#[tokio::test]
async fn child_start_spawns_process() {
    let mgr = ChildProcessManager::new(sleep_config(60));
    let status = mgr.start().await.unwrap();
    assert!(status.active);
    assert!(status.pid.is_some());

    // Clean up.
    mgr.stop().await.unwrap();
}

#[tokio::test]
async fn child_stop_terminates() {
    let mgr = ChildProcessManager::new(sleep_config(60));
    mgr.start().await.unwrap();

    let status = mgr.stop().await.unwrap();
    assert!(!status.active);
    assert!(status.pid.is_none());
}

#[tokio::test]
async fn child_restart_cycles() {
    let mgr = ChildProcessManager::new(sleep_config(60));
    let s1 = mgr.start().await.unwrap();
    let pid1 = s1.pid.unwrap();

    let s2 = mgr.restart().await.unwrap();
    assert!(s2.active);
    assert_ne!(s2.pid.unwrap(), pid1);

    mgr.stop().await.unwrap();
}

#[tokio::test]
async fn child_status_tracks_uptime() {
    let mgr = ChildProcessManager::new(sleep_config(60));
    mgr.start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = mgr.status().await.unwrap();
    assert!(status.active);
    // uptime_secs should be 0 (sub-second) — just verify it's present.
    assert!(status.uptime_secs.is_some());

    mgr.stop().await.unwrap();
}

#[tokio::test]
async fn child_logs_captures_stdout() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo hello; echo world".to_string(),
        ],
        max_restarts: 0,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    });
    mgr.start().await.unwrap();

    // Wait for the process to finish and output to be captured.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let lines = mgr.logs(10).await.unwrap();
    assert!(lines.contains(&"hello".to_string()));
    assert!(lines.contains(&"world".to_string()));
}

#[tokio::test]
async fn child_log_buffer_truncates() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "for i in $(seq 1 20); do echo line$i; done".to_string(),
        ],
        max_restarts: 0,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 5,
    });
    mgr.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let lines = mgr.logs(100).await.unwrap();
    assert!(
        lines.len() <= 5,
        "buffer should be capped at 5, got {}",
        lines.len()
    );
}

#[tokio::test]
async fn child_watchdog_auto_restarts() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec!["true".to_string()], // exits immediately with 0
        max_restarts: 2,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    });
    mgr.start().await.unwrap();

    // Wait enough time for the process to exit and restart a few times.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let st = mgr.state.read().await;
    assert!(
        st.restart_count > 0,
        "watchdog should have attempted restarts"
    );
}

#[tokio::test]
async fn child_watchdog_gives_up() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec!["false".to_string()], // exits immediately with 1
        max_restarts: 1,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    });
    mgr.start().await.unwrap();

    // Wait for watchdog to exhaust restarts.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let status = mgr.status().await.unwrap();
    assert!(
        !status.active,
        "should be inactive after max restarts exceeded"
    );
}

#[tokio::test]
async fn child_stop_prevents_watchdog() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec!["sleep".to_string(), "60".to_string()],
        max_restarts: 5,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    });
    mgr.start().await.unwrap();

    // Intentional stop should disable watchdog.
    mgr.stop().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let status = mgr.status().await.unwrap();
    assert!(!status.active, "should stay stopped (no watchdog restart)");
}

#[tokio::test]
async fn child_double_start_noop() {
    let mgr = ChildProcessManager::new(sleep_config(60));
    let s1 = mgr.start().await.unwrap();
    let s2 = mgr.start().await.unwrap();

    // Second start should return same pid.
    assert_eq!(s1.pid, s2.pid);

    mgr.stop().await.unwrap();
}

#[tokio::test]
async fn shell_words_parsing() {
    let input = r#"cargo run --release -- live --db-path "/tmp/my db.db""#;
    let words = shell_words::split(input).unwrap();
    assert_eq!(words[0], "cargo");
    assert_eq!(words[2], "--release");
    assert_eq!(words[5], "--db-path");
    assert_eq!(words[6], "/tmp/my db.db");
}

// -- Edge case tests ----------------------------------------------------------

#[tokio::test]
async fn child_captures_stderr() {
    let mgr = ChildProcessManager::new(ProcessConfig {
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo err_msg >&2".to_string(),
        ],
        max_restarts: 0,
        restart_delay: Duration::from_millis(50),
        log_buffer_size: 100,
    });
    mgr.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let lines = mgr.logs(10).await.unwrap();
    // stderr is captured via merged stdout+stderr or separate — check it shows up.
    let has_err = lines.iter().any(|l| l.contains("err_msg"));
    assert!(has_err, "stderr should be captured in logs: {lines:?}");
}

#[tokio::test]
async fn noop_logs_nonexistent_file_returns_error() {
    let mgr = NoopProcessManager::new(Some("/nonexistent/path/bot.log".to_string()));
    let result = mgr.logs(100).await;
    assert!(result.is_err(), "reading nonexistent log file should fail");
}

#[tokio::test]
async fn noop_logs_empty_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.log");
    std::fs::write(&path, "").unwrap();

    let mgr = NoopProcessManager::new(Some(path.to_str().unwrap().to_string()));
    let lines = mgr.logs(10).await.unwrap();
    assert!(lines.is_empty(), "empty file should return empty vec");
}

#[tokio::test]
async fn noop_logs_fewer_lines_than_requested() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.log");
    std::fs::write(&path, "one\ntwo\n").unwrap();

    let mgr = NoopProcessManager::new(Some(path.to_str().unwrap().to_string()));
    let lines = mgr.logs(1000).await.unwrap();
    assert_eq!(lines.len(), 2);
}

#[tokio::test]
async fn noop_logs_zero_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

    let mgr = NoopProcessManager::new(Some(path.to_str().unwrap().to_string()));
    let lines = mgr.logs(0).await.unwrap();
    assert!(lines.is_empty(), "requesting 0 lines should return empty");
}
