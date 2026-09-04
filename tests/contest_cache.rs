//! Integration test for contest caching + offline fallback.
//! There is no UI to drive, so we exercise the feature the way the real client
//! does: stand up the in-process mock server, point a real `ContestManager` at
//! it over HTTP, then kill the server to simulate going offline and assert the
//! cached copy is still served.
//!
//! This lives in its own test binary (its own `tests/*.rs` file) and holds a
//! SINGLE test on purpose: it overrides the `HOME` environment variable to
//! redirect the manager's `~/.cache/cp-client` at a throwaway directory. Env
//! vars are process-global and `cargo` runs tests within one binary on parallel
//! threads, so a second test here could race on `HOME`. One test = no race.

use std::net::TcpListener;
use std::time::Duration;

use cp_client::config::Config;
use cp_client::contest::manager::ContestManager;
use cp_client::mock_server::run_mock_server;
use tempfile::TempDir;

/// Grabs a free TCP port from the OS, then releases it so the mock server can
/// bind it. A tiny race window exists between release and re-bind, which is
/// acceptable for a local, single-process test.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("read local addr")
        .port()
}

async fn wait_until_listening(port: u16) {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("mock server never started listening on port {port}");
}

async fn wait_until_down(port: u16) {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("mock server never stopped listening on port {port}");
}

#[tokio::test]
async fn contest_cache_survives_going_offline() {
    // Redirect the manager's cache dir into a temp folder so the test neither
    // reads nor pollutes the developer's real ~/.cache/cp-client.
    let cache_home = TempDir::new().unwrap();
    std::env::set_var("HOME", cache_home.path());
    std::env::set_var("USERPROFILE", cache_home.path());

    // Start the mock server on a free port and wait until it accepts connections.
    let port = free_port();
    let server = tokio::spawn(async move {
        let _ = run_mock_server(port).await;
    });
    wait_until_listening(port).await;

    let config = Config {
        server_url: format!("http://127.0.0.1:{port}"),
        cache_ttl_secs: 3600,
        ..Config::default()
    };

    // Phase 1 — Online: the fetch hits the server and writes the cache.
    let manager = ContestManager::new(&config).unwrap();
    let online = manager.fetch_contest("contest-101").await.unwrap();
    assert_eq!(online.title, "Mock Practice Round 1");
    assert_eq!(online.problems.len(), 2);

    let cache_file = cache_home
        .path()
        .join(".cache")
        .join("cp-client")
        .join("contest_contest-101.json");
    assert!(cache_file.exists(), "fetch should have written the cache file");

    
    // Phase 2 — Go offline: kill the server and confirm the port stops accepting.
    server.abort();
    wait_until_down(port).await;

    // Phase 3 — Offline hit: the network call now fails, so fetch must fall back
    //           to the still-fresh cache and return the same contest.
    let offline = manager.fetch_contest("contest-101").await.unwrap();
    assert_eq!(offline.title, online.title);
    assert_eq!(offline.id, "contest-101");

    // Phase 4 — Offline miss: a contest never cached has nothing to fall back
    //           to, so the error is surfaced rather than masked.
    let uncached = manager.fetch_contest("never-seen").await;
    assert!(uncached.is_err(), "no server and no cache should error");
}
