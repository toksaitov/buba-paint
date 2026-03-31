use super::*;

/// Verifies that hash and verify password.
#[test]
fn hash_and_verify_password() {
    let hash = hash_password("my-secret-pass").unwrap();
    assert!(verify_password("my-secret-pass", &hash));
    assert!(!verify_password("wrong-pass", &hash));
}

/// Verifies that hash produces different salts.
#[test]
fn hash_produces_different_salts() {
    let h1 = hash_password("same-pass").unwrap();
    let h2 = hash_password("same-pass").unwrap();

    assert_ne!(h1, h2);

    assert!(verify_password("same-pass", &h1));
    assert!(verify_password("same-pass", &h2));
}

/// Verifies that verify invalid hash format.
#[test]
fn verify_invalid_hash_format() {
    assert!(!verify_password("pass", "not-a-valid-hash"));
}

/// Verifies that create and validate jwt.
#[test]
fn create_and_validate_jwt() {
    let token = create_jwt("user-123", "admin", "jwt-secret", 3600);
    assert!(!token.is_empty());

    let claims = validate_jwt(&token, "jwt-secret").unwrap();
    assert_eq!(claims.sub, "user-123");
    assert_eq!(claims.role, "admin");
    assert!(claims.exp > claims.iat);
}

/// Verifies that validate jwt wrong secret.
#[test]
fn validate_jwt_wrong_secret() {
    let token = create_jwt("user-1", "admin", "correct-secret", 3600);
    let result = validate_jwt(&token, "wrong-secret");
    assert!(result.is_err());
}

/// Verifies that validate jwt expired.
#[test]
fn validate_jwt_expired() {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = Claims {
        sub: "user-1".to_string(),
        role: "admin".to_string(),
        exp: now - 10,
        iat: now - 100,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"secret"),
    )
    .unwrap();

    let result = validate_jwt(&token, "secret");
    assert!(result.is_err());
}

/// Verifies that validate jwt garbage token.
#[test]
fn validate_jwt_garbage_token() {
    let result = validate_jwt("not.a.jwt", "secret");
    assert!(result.is_err());
}

/// Verifies that jwt duration is respected.
#[test]
fn jwt_duration_is_respected() {
    let token = create_jwt("u1", "observer", "sec", 7200);
    let claims = validate_jwt(&token, "sec").unwrap();
    let diff = claims.exp - claims.iat;
    assert_eq!(diff, 7200);
}

/// Verifies that auth middleware allows login without token.
#[tokio::test]
async fn auth_middleware_allows_login_without_token() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::post;
    use tower::ServiceExt;

    let db =
        crate::db::DashboardDb::from_connection(rusqlite::Connection::open_in_memory().unwrap());

    let app = Router::new()
        .route("/api/auth/login", post(|| async { "ok" }))
        .route("/api/protected", post(|| async { "secret" }))
        .layer(middleware::from_fn(require_auth))
        .layer(axum::Extension(AuthState {
            jwt_secret: "s".to_string(),
            db: std::sync::Arc::new(db),
        }));

    let resp = app
        .clone()
        .oneshot(
            Request::post("/api/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .oneshot(Request::post("/api/protected").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that auth middleware accepts valid token.
#[tokio::test]
async fn auth_middleware_accepts_valid_token() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use tower::ServiceExt;

    let db =
        crate::db::DashboardDb::from_connection(rusqlite::Connection::open_in_memory().unwrap());

    let token = create_jwt("user-1", "admin", "test-secret", 3600);

    let app = Router::new()
        .route("/api/test", get(|| async { "protected-content" }))
        .layer(middleware::from_fn(require_auth))
        .layer(axum::Extension(AuthState {
            jwt_secret: "test-secret".to_string(),
            db: std::sync::Arc::new(db),
        }));

    let resp = app
        .oneshot(
            Request::get("/api/test")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
