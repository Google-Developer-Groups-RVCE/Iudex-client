//! End-to-end tests of the `login`, `register`, `whoami` and `logout`
//! commands, run as the real binary.
//!
//! Each test gives the process its own `HOME`, so the saved configuration is
//! isolated per test without touching this process's environment, and tests
//! can run in parallel.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use common::{spawn_stub, UNREACHABLE_URL};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

struct Home {
    dir: TempDir,
}

impl Home {
    /// A home whose config is already written, so the binary skips toolchain
    /// detection and starts from a known session.
    fn with_session(server_url: &str, token: Option<&str>, username: Option<&str>) -> Self {
        let home = Home {
            dir: TempDir::new().unwrap(),
        };
        std::fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        let config = json!({
            "server_url": server_url,
            "auth_token": token,
            "username": username,
            "cpp_compiler": "g++",
            "java_compiler": "javac",
            "java_runner": "java",
            "python_interpreter": "python3",
            "default_timeout_ms": 5000,
            "max_output_bytes": 10485760
        });
        std::fs::write(home.config_path(), config.to_string()).unwrap();
        home
    }

    fn config_path(&self) -> PathBuf {
        self.dir
            .path()
            .join(".config")
            .join("cp-client")
            .join("config.json")
    }

    fn config(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.config_path()).unwrap()).unwrap()
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

async fn run(home: &Home, args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cp-client"))
        .args(args)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut pipe = child.stdin.take().unwrap();
    pipe.write_all(stdin.as_bytes()).await.unwrap();
    drop(pipe);

    let Output {
        status,
        stdout,
        stderr,
    } = child.wait_with_output().await.unwrap();
    Run {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

const ALICE: &str = r#"{"token":"tok-alice","username":"alice","role":"CONTESTANT"}"#;

// ---- register ----------------------------------------------------------

#[tokio::test]
async fn register_saves_the_session_and_the_server() {
    let stub = spawn_stub(&[("POST", "/auth/register", 201, ALICE)]).await;
    let home = Home::with_session(UNREACHABLE_URL, None, None);

    let out = run(
        &home,
        &[
            "register",
            "-s",
            &stub.url,
            "-u",
            "alice",
            "--password-stdin",
        ],
        "secretpass\n",
    )
    .await;

    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("Account created. Logged in as 'alice' (contestant)."),
        "{}",
        out.stdout
    );
    let config = home.config();
    assert_eq!(config["server_url"], stub.url.as_str());
    assert_eq!(config["auth_token"], "tok-alice");
    assert_eq!(config["username"], "alice");

    let request = stub.single_request();
    assert_eq!(request.path, "/auth/register");
    let body: Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body, json!({"username": "alice", "password": "secretpass"}));
}

#[tokio::test]
async fn register_shows_validation_errors_by_field() {
    let stub = spawn_stub(&[(
        "POST",
        "/auth/register",
        400,
        r#"{"error":"Validation failed","fields":{"password":"password must be between 8 and 72 characters"}}"#,
    )])
    .await;
    let home = Home::with_session(&stub.url, None, None);

    let out = run(
        &home,
        &["register", "-u", "alice", "--password-stdin"],
        "short\n",
    )
    .await;

    assert!(!out.success);
    assert!(
        out.stderr
            .contains("password: password must be between 8 and 72 characters"),
        "{}",
        out.stderr
    );
    assert_eq!(home.config()["auth_token"], Value::Null);
}

// ---- login -------------------------------------------------------------

/// `--server` used to default to localhost, silently resetting a saved server.
#[tokio::test]
async fn login_without_server_flag_uses_the_saved_server() {
    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;
    let home = Home::with_session(&stub.url, None, None);

    let out = run(
        &home,
        &["login", "-u", "alice", "--password-stdin"],
        "secretpass\n",
    )
    .await;

    assert!(out.success, "{}", out.stderr);
    assert_eq!(stub.single_request().path, "/auth/login");
    assert_eq!(home.config()["server_url"], stub.url.as_str());
    assert_eq!(home.config()["auth_token"], "tok-alice");
    assert!(out.stdout.contains("(contestant)"), "{}", out.stdout);
}

#[tokio::test]
async fn password_from_stdin_drops_the_line_ending() {
    for input in ["secretpass\n", "secretpass\r\n", "secretpass"] {
        let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;
        let home = Home::with_session(&stub.url, None, None);

        let out = run(&home, &["login", "-u", "alice", "--password-stdin"], input).await;

        assert!(out.success, "{input:?}: {}", out.stderr);
        let body: Value = serde_json::from_str(&stub.single_request().body).unwrap();
        assert_eq!(body["password"], "secretpass", "input {input:?}");
    }
}

#[tokio::test]
async fn empty_password_is_refused_before_contacting_the_server() {
    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;
    let home = Home::with_session(&stub.url, None, None);

    let out = run(&home, &["login", "-u", "alice", "--password-stdin"], "\n").await;

    assert!(!out.success);
    assert!(
        out.stderr.contains("Password must not be empty"),
        "{}",
        out.stderr
    );
    assert!(stub.requests().is_empty());
}

/// A failed login must leave the previous session, and its server, intact.
#[tokio::test]
async fn failed_login_keeps_the_previous_session() {
    let other_server = spawn_stub(&[(
        "POST",
        "/auth/login",
        401,
        r#"{"error":"Invalid username or password"}"#,
    )])
    .await;
    let home = Home::with_session(UNREACHABLE_URL, Some("old-token"), Some("old_user"));

    let out = run(
        &home,
        &[
            "login",
            "-s",
            &other_server.url,
            "-u",
            "alice",
            "--password-stdin",
        ],
        "wrongpass\n",
    )
    .await;

    assert!(!out.success);
    let config = home.config();
    assert_eq!(config["server_url"], UNREACHABLE_URL);
    assert_eq!(config["auth_token"], "old-token");
    assert_eq!(config["username"], "old_user");
    // The reason appears once; it used to be prefixed with "Login failed" twice.
    assert_eq!(
        out.stderr.matches("Invalid username or password").count(),
        1,
        "{}",
        out.stderr
    );
    assert_eq!(
        out.stderr.matches("Login failed").count(),
        1,
        "{}",
        out.stderr
    );
}

#[tokio::test]
async fn password_flag_is_gone() {
    let home = Home::with_session(UNREACHABLE_URL, None, None);

    let out = run(&home, &["login", "-u", "alice", "--password", "secret"], "").await;

    assert!(!out.success);
    assert!(
        out.stderr.contains("unexpected argument '--password'"),
        "{}",
        out.stderr
    );
}

// ---- whoami ------------------------------------------------------------

#[tokio::test]
async fn whoami_shows_the_user_role_and_server() {
    let stub = spawn_stub(&[(
        "GET",
        "/auth/me",
        200,
        r#"{"username":"alice","role":"CONTESTMASTER"}"#,
    )])
    .await;
    let home = Home::with_session(&stub.url, Some("tok-alice"), Some("alice"));

    let out = run(&home, &["whoami"], "").await;

    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout.contains("Logged in as 'alice' (contestmaster)"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains(&stub.url), "{}", out.stdout);
    assert_eq!(
        stub.single_request().authorization.as_deref(),
        Some("Bearer tok-alice")
    );
}

#[tokio::test]
async fn whoami_without_a_session_says_to_log_in() {
    let stub = spawn_stub(&[("GET", "/auth/me", 401, "")]).await;
    let home = Home::with_session(&stub.url, None, None);

    let out = run(&home, &["whoami"], "").await;

    assert!(!out.success);
    assert!(
        out.stderr
            .contains("You are not logged in. Run `cp-client login` first."),
        "{}",
        out.stderr
    );
}

#[tokio::test]
async fn whoami_with_a_revoked_session_says_to_log_in_again() {
    let stub = spawn_stub(&[("GET", "/auth/me", 401, "")]).await;
    let home = Home::with_session(&stub.url, Some("revoked"), Some("alice"));

    let out = run(&home, &["whoami"], "").await;

    assert!(!out.success);
    assert!(
        out.stderr.contains("rejected your saved session"),
        "{}",
        out.stderr
    );
}

// ---- logout ------------------------------------------------------------

#[tokio::test]
async fn logout_revokes_on_the_server_and_forgets_locally() {
    let stub = spawn_stub(&[("POST", "/auth/logout", 204, "")]).await;
    let home = Home::with_session(&stub.url, Some("tok-alice"), Some("alice"));

    let out = run(&home, &["logout"], "").await;

    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("Logged out 'alice'. Sessions on every device"),
        "{}",
        out.stdout
    );
    let request = stub.single_request();
    assert_eq!(request.path, "/auth/logout");
    assert_eq!(request.authorization.as_deref(), Some("Bearer tok-alice"));

    let config = home.config();
    assert_eq!(config["auth_token"], Value::Null);
    assert_eq!(config["username"], Value::Null);
    // Only the session is dropped; the server stays configured.
    assert_eq!(config["server_url"], stub.url.as_str());
}

#[tokio::test]
async fn logout_of_an_already_revoked_session_still_succeeds() {
    let stub = spawn_stub(&[("POST", "/auth/logout", 401, "")]).await;
    let home = Home::with_session(&stub.url, Some("revoked"), Some("alice"));

    let out = run(&home, &["logout"], "").await;

    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("The server had already ended this session."),
        "{}",
        out.stdout
    );
    assert_eq!(home.config()["auth_token"], Value::Null);
}

/// Asking to be signed out must work even when the server cannot be reached.
#[tokio::test]
async fn logout_with_the_server_down_forgets_locally_and_warns() {
    let home = Home::with_session(UNREACHABLE_URL, Some("tok-alice"), Some("alice"));

    let out = run(&home, &["logout"], "").await;

    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout.contains("Logged out 'alice' on this machine."),
        "{}",
        out.stdout
    );
    assert!(
        out.stderr
            .contains("could not revoke the session on the server"),
        "{}",
        out.stderr
    );
    assert_eq!(home.config()["auth_token"], Value::Null);
    assert_eq!(home.config()["username"], Value::Null);
}

#[tokio::test]
async fn logout_when_not_logged_in_contacts_nobody() {
    let stub = spawn_stub(&[("POST", "/auth/logout", 204, "")]).await;
    let home = Home::with_session(&stub.url, None, None);

    let out = run(&home, &["logout"], "").await;

    assert!(out.success, "{}", out.stderr);
    assert!(out.stdout.contains("Not logged in."), "{}", out.stdout);
    assert!(stub.requests().is_empty());
}

#[tokio::test]
async fn logout_then_whoami_reports_not_logged_in() {
    let stub = spawn_stub(&[
        ("POST", "/auth/logout", 204, ""),
        ("GET", "/auth/me", 401, ""),
    ])
    .await;
    let home = Home::with_session(&stub.url, Some("tok-alice"), Some("alice"));

    assert!(run(&home, &["logout"], "").await.success);
    let out = run(&home, &["whoami"], "").await;

    assert!(!out.success);
    assert!(
        out.stderr.contains("You are not logged in"),
        "{}",
        out.stderr
    );
    // The follow-up request carried no stale token.
    assert_eq!(stub.requests()[1].authorization, None);
}

// ---- config file -------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn saved_session_is_readable_only_by_the_owner() {
    use std::os::unix::fs::PermissionsExt;

    let stub = spawn_stub(&[("POST", "/auth/login", 200, ALICE)]).await;
    let home = Home::with_session(&stub.url, None, None);

    assert!(
        run(
            &home,
            &["login", "-u", "alice", "--password-stdin"],
            "secretpass\n"
        )
        .await
        .success
    );

    let mode = std::fs::metadata(home.config_path())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
