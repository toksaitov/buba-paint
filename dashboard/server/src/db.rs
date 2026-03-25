use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use crate::error::DashboardError;

/// Dashboard database — manages users and sessions.
pub struct DashboardDb {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub created_at: u64,
    pub expires_at: u64,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY,
    username   TEXT NOT NULL UNIQUE,
    password   TEXT NOT NULL,
    role       TEXT NOT NULL DEFAULT 'observer' CHECK(role IN ('admin','observer')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    token      TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
";

impl DashboardDb {
    /// Open or create the dashboard database.
    pub fn new(db_path: &str) -> Result<Self, DashboardError> {
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()
        } else {
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        DashboardError::Internal(format!("creating db directory: {e}"))
                    })?;
                }
            }
            Connection::open(db_path)
        }
        .map_err(|e| DashboardError::Internal(format!("opening database: {e}")))?;

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(DashboardError::Database)?;
        conn.execute_batch(SCHEMA)
            .map_err(DashboardError::Database)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create from an existing connection (for testing).
    #[cfg(test)]
    pub fn from_connection(conn: Connection) -> Self {
        conn.execute_batch(SCHEMA).unwrap();
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Seed an admin user if no users exist.
    pub async fn seed_admin(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<(), DashboardError> {
        let conn = self.conn.lock().await;
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;

        if count == 0 {
            let now = now_ms();
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO users (id, username, password, role, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 'admin', ?4, ?5)",
                params![id, username, password_hash, now, now],
            )?;
            tracing::info!("seeded admin user: {username}");
        }

        Ok(())
    }

    /// Create a new user.
    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<User, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO users (id, username, password, role, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, username, password_hash, role, now, now],
        )?;

        Ok(User {
            id,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            role: role.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Get a user by username.
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, username, password, role, created_at, updated_at FROM users WHERE username = ?1",
            params![username],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(user) => Ok(Some(user)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// Get a user by ID.
    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, username, password, role, created_at, updated_at FROM users WHERE id = ?1",
            params![id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(user) => Ok(Some(user)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// List all users.
    pub async fn list_users(&self) -> Result<Vec<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, password, role, created_at, updated_at FROM users ORDER BY created_at",
        )?;

        let users = stmt
            .query_map([], |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(users)
    }

    /// Create a session.
    pub async fn create_session(
        &self,
        user_id: &str,
        token: &str,
        expires_at: u64,
    ) -> Result<Session, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO sessions (id, user_id, token, created_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, user_id, token, now, expires_at],
        )?;

        Ok(Session {
            id,
            user_id: user_id.to_string(),
            token: token.to_string(),
            created_at: now,
            expires_at,
        })
    }

    /// Get a session by token.
    pub async fn get_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<Session>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, user_id, token, created_at, expires_at FROM sessions WHERE token = ?1",
            params![token],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    token: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// Delete a session by token.
    pub async fn delete_session(&self, token: &str) -> Result<(), DashboardError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/db_tests.rs"]
mod tests;
