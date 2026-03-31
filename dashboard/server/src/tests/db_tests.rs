use rusqlite::Connection;

use super::*;

/// Test db.
fn test_db() -> DashboardDb {
    DashboardDb::from_connection(Connection::open_in_memory().unwrap())
}

/// Verifies that seed admin creates user when empty.
#[tokio::test]
async fn seed_admin_creates_user_when_empty() {
    let db = test_db();
    db.seed_admin("admin", "$argon2id$hash").await.unwrap();

    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    assert_eq!(user.username, "admin");
    assert_eq!(user.role, "admin");
}

/// Verifies that seed admin skips when users exist.
#[tokio::test]
async fn seed_admin_skips_when_users_exist() {
    let db = test_db();
    db.create_user("existing", "hash", "observer")
        .await
        .unwrap();

    db.seed_admin("admin", "hash").await.unwrap();

    let admin = db.get_user_by_username("admin").await.unwrap();
    assert!(admin.is_none());
}

/// Verifies that create user and retrieve by username.
#[tokio::test]
async fn create_user_and_retrieve_by_username() {
    let db = test_db();
    let user = db
        .create_user("alice", "hash123", "observer")
        .await
        .unwrap();

    assert_eq!(user.username, "alice");
    assert_eq!(user.role, "observer");
    assert!(!user.id.is_empty());

    let found = db.get_user_by_username("alice").await.unwrap().unwrap();
    assert_eq!(found.id, user.id);
}

/// Verifies that create user and retrieve by id.
#[tokio::test]
async fn create_user_and_retrieve_by_id() {
    let db = test_db();
    let user = db.create_user("bob", "hash456", "admin").await.unwrap();

    let found = db.get_user_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(found.username, "bob");
    assert_eq!(found.role, "admin");
}

/// Verifies that get user by username not found.
#[tokio::test]
async fn get_user_by_username_not_found() {
    let db = test_db();
    let found = db.get_user_by_username("nonexistent").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that get user by id not found.
#[tokio::test]
async fn get_user_by_id_not_found() {
    let db = test_db();
    let found = db.get_user_by_id("nonexistent-id").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that list users empty.
#[tokio::test]
async fn list_users_empty() {
    let db = test_db();
    let users = db.list_users().await.unwrap();
    assert!(users.is_empty());
}

/// Verifies that list users returns all.
#[tokio::test]
async fn list_users_returns_all() {
    let db = test_db();
    db.create_user("alice", "h1", "admin").await.unwrap();
    db.create_user("bob", "h2", "observer").await.unwrap();

    let users = db.list_users().await.unwrap();
    assert_eq!(users.len(), 2);
}

/// Verifies that create session and retrieve by token.
#[tokio::test]
async fn create_session_and_retrieve_by_token() {
    let db = test_db();
    let user = db.create_user("carol", "h3", "observer").await.unwrap();

    let session = db
        .create_session(&user.id, "jwt-token-123", 9_999_999_999_999)
        .await
        .unwrap();

    assert_eq!(session.user_id, user.id);
    assert_eq!(session.token, "jwt-token-123");

    let found = db
        .get_session_by_token("jwt-token-123")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.user_id, user.id);
}

/// Verifies that get session by token not found.
#[tokio::test]
async fn get_session_by_token_not_found() {
    let db = test_db();
    let found = db.get_session_by_token("nonexistent").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that delete session.
#[tokio::test]
async fn delete_session() {
    let db = test_db();
    let user = db.create_user("dan", "h4", "observer").await.unwrap();
    db.create_session(&user.id, "token-del", 9_999_999_999_999)
        .await
        .unwrap();

    db.delete_session("token-del").await.unwrap();

    let found = db.get_session_by_token("token-del").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that duplicate username fails.
#[tokio::test]
async fn duplicate_username_fails() {
    let db = test_db();
    db.create_user("dup", "h1", "observer").await.unwrap();

    let result = db.create_user("dup", "h2", "observer").await;
    assert!(result.is_err());
}

/// Verifies that user password hash not serialized.
#[tokio::test]
async fn user_password_hash_not_serialized() {
    let db = test_db();
    let user = db
        .create_user("eve", "secret-hash", "observer")
        .await
        .unwrap();

    let json = serde_json::to_string(&user).unwrap();
    assert!(!json.contains("secret-hash"));
}
