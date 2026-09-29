//! Contract tests for the auth calls in `ApiClient`.
//!
//! The stub answers with the bodies the real Spring server produces, so these
//! pin the paths, headers and JSON shapes the two sides agree on.

mod common;

use common::spawn_stub;
use cp_client::api::client::ApiClient;
use cp_client::api::models::Role;
use cp_client::config::Config;
use cp_client::error::ClientError;

fn client(url: &str, token: Option<&str>) -> ApiClient {
    ApiClient::new(&Config {
        server_url: url.to_string(),
        auth_token: token.map(str::to_string),
        ..Config::default()
    })
}

const ALICE: &str = r#"{"token":"tok-alice","username":"alice","role":"CONTESTANT"}"#;

#[tokio::test]
async fn login_posts_credentials_to_auth_login() {
    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;

    let response = client(&stub.url, None)
        .login("alice", "secretpass")
        .await
        .unwrap();

    assert_eq!(response.token, "tok-alice");
    assert_eq!(response.username, "alice");
    assert_eq!(response.role, Role::Contestant);

    let request = stub.single_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/auth/login");
    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"username": "alice", "password": "secretpass"})
    );
}

/// Credentials must not travel with a stale token from an earlier session.
#[tokio::test]
async fn login_sends_no_authorization_header() {
    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;

    client(&stub.url, Some("old-token"))
        .login("alice", "secretpass")
        .await
        .unwrap();

    assert_eq!(stub.single_request().authorization, None);
}

#[tokio::test]
async fn trailing_slash_in_server_url_is_ignored() {
    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;

    client(&format!("{}/", stub.url), None)
        .login("alice", "secretpass")
        .await
        .unwrap();

    assert_eq!(stub.single_request().path, "/auth/login");
}

#[tokio::test]
async fn register_posts_to_auth_register_and_accepts_created() {
    let stub = spawn_stub(&[("POST", "/auth/register", 201, ALICE)]).await;

    let response = client(&stub.url, None)
        .register("alice", "secretpass")
        .await
        .unwrap();

    assert_eq!(response.token, "tok-alice");
    assert_eq!(response.role, Role::Contestant);
    let request = stub.single_request();
    assert_eq!(request.path, "/auth/register");
    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"username": "alice", "password": "secretpass"})
    );
}

#[tokio::test]
async fn every_server_role_is_understood() {
    for (wire, role) in [
        ("ADMIN", Role::Admin),
        ("CONTESTMASTER", Role::Contestmaster),
        ("CONTESTANT", Role::Contestant),
        ("JUDGE", Role::Unknown),
    ] {
        let body = format!(r#"{{"token":"t","username":"u","role":"{wire}"}}"#);
        let stub = spawn_stub(&[("POST", "/auth/login", 200, &body)]).await;

        let response = client(&stub.url, None).login("u", "pw").await.unwrap();

        assert_eq!(response.role, role, "role {wire}");
    }
}

/// A 401 from login means wrong credentials, not an expired session.
#[tokio::test]
async fn rejected_login_reports_the_servers_reason() {
    let stub = spawn_stub(&[(
        "POST",
        "/auth/login",
        401,
        r#"{"error":"Invalid username or password"}"#,
    )])
    .await;

    let err = client(&stub.url, None)
        .login("alice", "wrong")
        .await
        .unwrap_err();

    match err {
        ClientError::ApiError {
            message,
            status_code,
        } => {
            assert_eq!(message, "Invalid username or password");
            assert_eq!(status_code, Some(401));
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

#[tokio::test]
async fn registration_validation_errors_name_each_field() {
    let stub = spawn_stub(&[(
        "POST",
        "/auth/register",
        400,
        r#"{"error":"Validation failed","fields":{"password":"password must be between 8 and 72 characters"}}"#,
    )])
    .await;

    let err = client(&stub.url, None)
        .register("alice", "short")
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("Validation failed (password: password must be between 8 and 72 characters)"),
        "{err}"
    );
}

#[tokio::test]
async fn taken_username_reports_the_conflict() {
    let stub = spawn_stub(&[(
        "POST",
        "/auth/register",
        409,
        r#"{"error":"Username already exists: alice"}"#,
    )])
    .await;

    let err = client(&stub.url, None)
        .register("alice", "secretpass")
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ClientError::ApiError { message, status_code: Some(409) }
            if message == "Username already exists: alice"),
        "{err:?}"
    );
}

#[tokio::test]
async fn unreachable_server_is_a_network_error() {
    let err = client(common::UNREACHABLE_URL, None)
        .login("alice", "secretpass")
        .await
        .unwrap_err();

    assert!(matches!(err, ClientError::Network(_)), "{err:?}");
}

#[tokio::test]
async fn me_sends_the_bearer_token_and_reads_the_role() {
    let stub = spawn_stub(&[(
        "GET",
        "/auth/me",
        200,
        r#"{"username":"alice","role":"CONTESTMASTER"}"#,
    )])
    .await;

    let me = client(&stub.url, Some("tok-alice")).me().await.unwrap();

    assert_eq!(me.username, "alice");
    assert_eq!(me.role, Some(Role::Contestmaster));
    let request = stub.single_request();
    assert_eq!(request.path, "/auth/me");
    assert_eq!(request.authorization.as_deref(), Some("Bearer tok-alice"));
}

/// The server sends `"role": null` for an account with no authority.
#[tokio::test]
async fn me_accepts_a_null_role() {
    let stub = spawn_stub(&[(
        "GET",
        "/auth/me",
        200,
        r#"{"username":"alice","role":null}"#,
    )])
    .await;

    let me = client(&stub.url, Some("t")).me().await.unwrap();

    assert_eq!(me.role, None);
}

/// Spring Security's 401 has no body; the client must supply the explanation.
#[tokio::test]
async fn unauthorized_without_a_token_says_not_logged_in() {
    let stub = spawn_stub(&[("GET", "/auth/me", 401, "")]).await;

    let err = client(&stub.url, None).me().await.unwrap_err();

    match err {
        ClientError::Unauthorized(message) => {
            assert!(message.contains("not logged in"), "{message}")
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
    assert_eq!(stub.single_request().authorization, None);
}

#[tokio::test]
async fn unauthorized_with_a_token_says_the_session_was_rejected() {
    let stub = spawn_stub(&[("GET", "/auth/me", 401, "")]).await;

    let err = client(&stub.url, Some("revoked")).me().await.unwrap_err();

    match err {
        ClientError::Unauthorized(message) => {
            assert!(message.contains("rejected your saved session"), "{message}");
            assert!(message.contains("logout on another device"), "{message}");
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn logout_posts_the_bearer_token_and_accepts_no_content() {
    let stub = spawn_stub(&[("POST", "/auth/logout", 204, "")]).await;

    client(&stub.url, Some("tok-alice")).logout().await.unwrap();

    let request = stub.single_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/auth/logout");
    assert_eq!(request.authorization.as_deref(), Some("Bearer tok-alice"));
}

#[tokio::test]
async fn logout_with_a_revoked_token_is_unauthorized() {
    let stub = spawn_stub(&[("POST", "/auth/logout", 401, "")]).await;

    let err = client(&stub.url, Some("revoked"))
        .logout()
        .await
        .unwrap_err();

    assert!(matches!(err, ClientError::Unauthorized(_)), "{err:?}");
}

/// Every API call shares the 401 handling, not just the auth ones.
#[tokio::test]
async fn data_endpoints_report_unauthorized_too() {
    let stub = spawn_stub(&[("GET", "/api/contests", 401, "")]).await;

    let err = client(&stub.url, Some("expired"))
        .get_contests()
        .await
        .unwrap_err();

    assert!(matches!(err, ClientError::Unauthorized(_)), "{err:?}");
}

#[tokio::test]
async fn data_endpoint_errors_use_the_servers_message() {
    let stub = spawn_stub(&[(
        "GET",
        "/api/contests/c1",
        403,
        r#"{"error":"You are not registered for this contest"}"#,
    )])
    .await;

    let err = client(&stub.url, Some("t"))
        .get_contest("c1")
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ClientError::ApiError { message, status_code: Some(403) }
            if message == "You are not registered for this contest"),
        "{err:?}"
    );
}
