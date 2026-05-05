use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::error::AgentError;
use crate::types::BotProcessStatus;

/// Trait for managing the bot process lifecycle.
///
/// Uses `Pin<Box<dyn Future>>` return types for object safety so the trait
/// can be used behind `Arc<dyn ProcessManager>`.
pub trait ProcessManager: Send + Sync {
    /// Starts the managed bot process and returns the resulting status.
    fn start(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>>;

    /// Stops the managed bot process and returns the resulting status.
    fn stop(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>>;

    /// Restarts the managed bot process and returns the resulting status.
    fn restart(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>>;

    /// Returns the current process status without mutating the process.
    fn status(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>>;

    /// Returns the newest captured process log lines.
    fn logs(
        &self,
        lines: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>>;
}

/// Configuration for spawning the bot process.
pub struct ProcessConfig {
    /// Command + args parsed from the `--bot-cmd` shell string.
    pub command: Vec<String>,
    /// Maximum automatic restarts before giving up (default 5).
    pub max_restarts: u32,
    /// Delay between automatic restarts (default 3 s).
    pub restart_delay: Duration,
    /// Ring buffer capacity for captured log lines (default 10 000).
    pub log_buffer_size: usize,
}

struct ProcessState {
    pid: Option<u32>,
    started_at: Option<Instant>,
    restart_count: u32,
}

/// Manages the bot as a child process.
///
/// Captures stdout/stderr in a ring buffer, provides a watchdog that
/// auto-restarts the process on unexpected exit, and sends SIGTERM (Unix)
/// or `kill()` (Windows) for graceful shutdown.
pub struct ChildProcessManager {
    config: ProcessConfig,
    state: Arc<RwLock<ProcessState>>,
    log_buffer: Arc<Mutex<VecDeque<String>>>,
    /// Set to `false` before an intentional stop so the watchdog does not
    /// restart the process.
    watchdog_enabled: Arc<AtomicBool>,
    /// Set to `true` while the process is alive. The watchdog clears it on
    /// exit and `start()` sets it.
    alive: Arc<AtomicBool>,
}

impl ChildProcessManager {
    /// Creates a new `ChildProcessManager`.
    pub fn new(config: ProcessConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(ProcessState {
                pid: None,
                started_at: None,
                restart_count: 0,
            })),
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(config.log_buffer_size))),
            watchdog_enabled: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(false)),
            config,
        }
    }

    /// Internal: spawn the child process, log-capture tasks, and watchdog.
    async fn spawn(&self) -> Result<(), AgentError> {
        if self.config.command.is_empty() {
            return Err(AgentError::BotControl("bot command is empty".to_string()));
        }

        let program = &self.config.command[0];
        let args = &self.config.command[1..];

        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AgentError::BotControl(format!("failed to spawn bot: {e}")))?;

        let pid = child.id();

        {
            let mut st = self.state.write().await;
            st.pid = pid;
            st.started_at = Some(Instant::now());
        }
        self.alive.store(true, Ordering::SeqCst);
        self.watchdog_enabled.store(true, Ordering::SeqCst);

        if let Some(stdout) = child.stdout.take() {
            let buf = Arc::clone(&self.log_buffer);
            let cap = self.config.log_buffer_size;
            tokio::spawn(async move {
                capture_lines(BufReader::new(stdout), buf, cap).await;
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let buf = Arc::clone(&self.log_buffer);
            let cap = self.config.log_buffer_size;
            tokio::spawn(async move {
                capture_lines(BufReader::new(stderr), buf, cap).await;
            });
        }

        let state = Arc::clone(&self.state);
        let log_buffer = Arc::clone(&self.log_buffer);
        let watchdog_enabled = Arc::clone(&self.watchdog_enabled);
        let alive = Arc::clone(&self.alive);
        let max_restarts = self.config.max_restarts;
        let restart_delay = self.config.restart_delay;
        let command = self.config.command.clone();
        let log_buffer_size = self.config.log_buffer_size;

        tokio::spawn(async move {
            watchdog_loop(
                child,
                state,
                log_buffer,
                watchdog_enabled,
                alive,
                max_restarts,
                restart_delay,
                command,
                log_buffer_size,
            )
            .await;
        });

        Ok(())
    }
}

impl ProcessManager for ChildProcessManager {
    /// Starts the managed process if it is not already running.
    fn start(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async move {
            if self.alive.load(Ordering::SeqCst) {
                return self.status().await;
            }

            {
                let mut st = self.state.write().await;
                st.restart_count = 0;
            }

            self.spawn().await?;

            tokio::time::sleep(Duration::from_millis(50)).await;

            self.status().await
        })
    }

    /// Stops the managed process and waits briefly for it to exit.
    fn stop(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async move {
            self.watchdog_enabled.store(false, Ordering::SeqCst);

            let pid = {
                let st = self.state.read().await;
                st.pid
            };

            if let Some(pid) = pid
                && self.alive.load(Ordering::SeqCst)
            {
                send_terminate(pid);

                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline && self.alive.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                if self.alive.load(Ordering::SeqCst) {
                    send_kill(pid);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }

            self.status().await
        })
    }

    /// Stops the managed process and starts it again.
    fn restart(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async move {
            self.stop().await?;

            tokio::time::sleep(Duration::from_millis(100)).await;
            self.start().await
        })
    }

    /// Returns the current child-process status snapshot.
    fn status(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async move {
            let st = self.state.read().await;
            let active = self.alive.load(Ordering::SeqCst);
            let uptime_secs = if active {
                st.started_at.map(|s| s.elapsed().as_secs())
            } else {
                None
            };
            Ok(BotProcessStatus {
                active,
                pid: if active { st.pid } else { None },
                uptime_secs,
                control_available: true,
            })
        })
    }

    /// Returns the newest captured log lines from the child process.
    fn logs(
        &self,
        lines: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>> {
        Box::pin(async move {
            let buf = self
                .log_buffer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let n = (lines as usize).min(buf.len());
            let start = buf.len() - n;
            Ok(buf.range(start..).cloned().collect())
        })
    }
}

/// Watchdog loop.
#[allow(clippy::too_many_arguments)]
async fn watchdog_loop(
    mut child: tokio::process::Child,
    state: Arc<RwLock<ProcessState>>,
    log_buffer: Arc<Mutex<VecDeque<String>>>,
    watchdog_enabled: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    max_restarts: u32,
    restart_delay: Duration,
    command: Vec<String>,
    log_buffer_size: usize,
) {
    loop {
        let _ = child.wait().await;
        alive.store(false, Ordering::SeqCst);

        if !watchdog_enabled.load(Ordering::SeqCst) {
            return;
        }

        let count = {
            let mut st = state.write().await;
            st.restart_count += 1;
            st.restart_count
        };

        if count > max_restarts {
            tracing::error!(
                "bot exited unexpectedly {count} times — giving up (max_restarts={max_restarts})"
            );
            push_log(
                &log_buffer,
                log_buffer_size,
                format!("[agent] bot exited — max restarts ({max_restarts}) exceeded, giving up"),
            );
            return;
        }

        tracing::warn!(
            "bot exited unexpectedly (restart {count}/{max_restarts}), restarting in {restart_delay:?}"
        );
        push_log(
            &log_buffer,
            log_buffer_size,
            format!(
                "[agent] bot exited — restarting ({count}/{max_restarts}) in {restart_delay:?}"
            ),
        );

        tokio::time::sleep(restart_delay).await;

        if !watchdog_enabled.load(Ordering::SeqCst) {
            return;
        }

        let program = &command[0];
        let args = &command[1..];
        match tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut new_child) => {
                let pid = new_child.id();
                {
                    let mut st = state.write().await;
                    st.pid = pid;
                    st.started_at = Some(Instant::now());
                }
                alive.store(true, Ordering::SeqCst);

                if let Some(stdout) = new_child.stdout.take() {
                    let buf = Arc::clone(&log_buffer);
                    tokio::spawn(async move {
                        capture_lines(BufReader::new(stdout), buf, log_buffer_size).await;
                    });
                }
                if let Some(stderr) = new_child.stderr.take() {
                    let buf = Arc::clone(&log_buffer);
                    tokio::spawn(async move {
                        capture_lines(BufReader::new(stderr), buf, log_buffer_size).await;
                    });
                }

                child = new_child;
            }
            Err(e) => {
                tracing::error!("failed to respawn bot: {e}");
                push_log(
                    &log_buffer,
                    log_buffer_size,
                    format!("[agent] failed to respawn bot: {e}"),
                );
                return;
            }
        }
    }
}

/// Captures lines.
async fn capture_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>,
    buffer: Arc<Mutex<VecDeque<String>>>,
    cap: usize,
) {
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        push_log(&buffer, cap, line);
    }
}

/// Push log.
fn push_log(buffer: &Mutex<VecDeque<String>>, cap: usize, line: String) {
    let mut buf = buffer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if buf.len() >= cap {
        buf.pop_front();
    }
    buf.push_back(line);
}

/// Sends terminate.
#[cfg(unix)]
fn send_terminate(pid: u32) {
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(i32::try_from(pid).expect("pid fits in i32"), libc::SIGTERM);
    }
}

/// Sends terminate.
#[cfg(not(unix))]
fn send_terminate(_pid: u32) {
    tracing::debug!("SIGTERM not available on this platform — relying on kill()");
}

/// Sends kill.
#[cfg(unix)]
fn send_kill(pid: u32) {
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(i32::try_from(pid).expect("pid fits in i32"), libc::SIGKILL);
    }
}

/// Sends kill.
#[cfg(not(unix))]
fn send_kill(_pid: u32) {
    tracing::debug!("SIGKILL not available on this platform");
}

/// Monitoring-only process manager. Does not control the bot process.
///
/// Useful when the bot is managed externally (systemd, Docker, another
/// supervisor, etc.). The agent still serves the REST/WS API and reads the
/// bot's `SQLite` database.
pub struct NoopProcessManager {
    log_path: Option<String>,
}

impl NoopProcessManager {
    /// Creates a new `NoopProcessManager`.
    pub fn new(log_path: Option<String>) -> Self {
        Self { log_path }
    }
}

impl ProcessManager for NoopProcessManager {
    /// Reports that bot control is unavailable in monitoring-only mode.
    fn start(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async {
            Err(AgentError::BotControlUnavailable(
                "process management disabled (monitor-only mode)".to_string(),
            ))
        })
    }

    /// Reports that bot control is unavailable in monitoring-only mode.
    fn stop(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async {
            Err(AgentError::BotControlUnavailable(
                "process management disabled (monitor-only mode)".to_string(),
            ))
        })
    }

    /// Reports that bot control is unavailable in monitoring-only mode.
    fn restart(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async {
            Err(AgentError::BotControlUnavailable(
                "process management disabled (monitor-only mode)".to_string(),
            ))
        })
    }

    /// Returns the static monitoring-only process status.
    fn status(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<BotProcessStatus, AgentError>> + Send + '_>> {
        Box::pin(async {
            Ok(BotProcessStatus {
                active: false,
                pid: None,
                uptime_secs: None,
                control_available: false,
            })
        })
    }

    /// Returns an empty log list in monitoring-only mode.
    fn logs(
        &self,
        lines: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(path) = &self.log_path {
                read_tail(path, lines).await
            } else {
                Ok(vec![
                    "No log source available (monitor-only mode)".to_string(),
                ])
            }
        })
    }
}

/// Read the last N lines from a file (like `tail -n`).
async fn read_tail(path: &str, n: u64) -> Result<Vec<String>, AgentError> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AgentError::BotControl(format!("failed to read log file {path}: {e}")))?;

    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(n as usize);
    Ok(all_lines[start..]
        .iter()
        .map(|s| (*s).to_string())
        .collect())
}

#[cfg(test)]
#[path = "tests/process_manager_tests.rs"]
mod tests;
