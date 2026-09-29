//! The mock server must answer on the same auth paths as the real one, or
//! local testing against it hides path mismatches (as `/api/auth/login` once
//! did).

use std::time::Duration;

use cp_client::api::client::ApiClient;
use cp_client::api::models::Role;
use cp_client::config::Config;
use cp_client::error::ClientError;
use cp_client::mock_server::run_mock_server;

async fn start_mock() -> String {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    tokio::spawn(async move {
        let _ = run_mock_server(port).await;
    });
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return format!("http://127.0.0.1:{port}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("mock server never started on port {port}");
}

fn client(url: &str, token: Option<String>) -> ApiClient {
    ApiClient::new(&Config {
        server_url: url.to_string(),
        auth_token: token,
        ..Config::default()
    })
}

#[tokio::test]
async fn mock_auth_round_trip() {
    let url = start_mock().await;

    let registered = client(&url, None)
        .register("alice", "secretpass")
        .await
        .unwrap();
    assert_eq!(registered.username, "alice");
    assert_eq!(registered.role, Role::Contestant);

    let logged_in = client(&url, None)
        .login("alice", "secretpass")
        .await
        .unwrap();
    assert!(!logged_in.token.is_empty());

    let session = client(&url, Some(logged_in.token));
    let me = session.me().await.unwrap();
    assert_eq!(me.username, "alice");
    assert_eq!(me.role, Some(Role::Contestant));

    session.logout().await.unwrap();
}

#[tokio::test]
async fn mock_rejects_missing_and_foreign_tokens() {
    let url = start_mock().await;

    for token in [None, Some("not-a-mock-token".to_string())] {
        let session = client(&url, token.clone());
        assert!(
            matches!(session.me().await, Err(ClientError::Unauthorized(_))),
            "me with {token:?}"
        );
        assert!(
            matches!(session.logout().await, Err(ClientError::Unauthorized(_))),
            "logout with {token:?}"
        );
    }
}
