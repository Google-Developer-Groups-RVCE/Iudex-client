//! A rejected session must surface, not be papered over by the offline cache.
//!
//! One test in its own binary, for the same reason as `contest_cache.rs`: it
//! points `HOME` at a temp directory, and env vars are process-global.

mod common;

use common::spawn_stub;
use cp_client::config::Config;
use cp_client::contest::manager::ContestManager;
use cp_client::error::ClientError;
use tempfile::TempDir;

const CACHED_PROBLEM: &str = r#"{
    "id": "P",
    "contest_id": "c1",
    "title": "Cached",
    "statement": "s",
    "input_spec": "i",
    "output_spec": "o",
    "constraints": "c",
    "time_limit_ms": 1000,
    "memory_limit_mb": 256
}"#;

const CACHED_TESTS: &str = r#"[{"id":"t1","input_data":"1\n"}]"#;

#[tokio::test]
async fn unauthorized_is_never_served_from_cache() {
    let home = TempDir::new().unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("USERPROFILE", home.path());

    // Fresh cache entries for both problems, so only the error kind decides
    // whether the cache is used.
    let cache_dir = home.path().join(".cache").join("cp-client");
    std::fs::create_dir_all(&cache_dir).unwrap();
    for id in ["DENIED", "DOWN"] {
        std::fs::write(cache_dir.join(format!("problem_{id}.json")), CACHED_PROBLEM).unwrap();
        std::fs::write(cache_dir.join(format!("tests_{id}.json")), CACHED_TESTS).unwrap();
    }

    let stub = spawn_stub(&[
        ("GET", "/api/problems/DENIED", 401, ""),
        ("GET", "/api/problems/DENIED/tests", 401, ""),
        (
            "GET",
            "/api/problems/DOWN",
            500,
            r#"{"error":"Storage failure"}"#,
        ),
        (
            "GET",
            "/api/problems/DOWN/tests",
            500,
            r#"{"error":"Storage failure"}"#,
        ),
    ])
    .await;

    let manager = ContestManager::new(&Config {
        server_url: stub.url.clone(),
        auth_token: Some("revoked".to_string()),
        cache_ttl_secs: 3600,
        ..Config::default()
    })
    .unwrap();

    // Revoked session: the error comes through despite a fresh cache.
    let problem = manager.fetch_problem("DENIED").await;
    assert!(
        matches!(problem, Err(ClientError::Unauthorized(_))),
        "{problem:?}"
    );
    let tests = manager.fetch_test_inputs("DENIED").await;
    assert!(
        matches!(tests, Err(ClientError::Unauthorized(_))),
        "{tests:?}"
    );

    // Any other failure still falls back, so the check above is about the
    // 401 specifically and not a broken cache.
    let problem = manager.fetch_problem("DOWN").await.unwrap();
    assert_eq!(problem.title, "Cached");
    let tests = manager.fetch_test_inputs("DOWN").await.unwrap();
    assert_eq!(tests.len(), 1);
}
