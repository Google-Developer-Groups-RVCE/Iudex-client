//! A scripted HTTP server for tests that need to see exactly what the client
//! sends and control exactly what it gets back.
//!
//! Unlike `mock_server`, which imitates a contest, this answers each
//! `(method, path)` with a fixed status and body, copied from what the real
//! Spring server returns, and records every request it receives.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: String,
}

type Routes = HashMap<(String, String), (u16, String)>;

#[derive(Clone)]
struct StubState {
    routes: Arc<Routes>,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

pub struct Stub {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl Stub {
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// The only request received; fails the test if there were more or fewer.
    pub fn single_request(&self) -> Recorded {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "expected one request, got {requests:?}");
        requests.into_iter().next().unwrap()
    }
}

/// Serves `(method, path, status, body)` routes on an OS-assigned port. An
/// unlisted route answers 404 with an empty body.
pub async fn spawn_stub(routes: &[(&str, &str, u16, &str)]) -> Stub {
    let routes: Routes = routes
        .iter()
        .map(|(method, path, status, body)| {
            (
                (method.to_string(), path.to_string()),
                (*status, body.to_string()),
            )
        })
        .collect();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let state = StubState {
        routes: Arc::new(routes),
        requests: requests.clone(),
    };

    let app = Router::new().fallback(respond).with_state(state);
    // Binding port 0 here, rather than picking a port and binding later,
    // leaves no window for another test to take it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Stub { url, requests }
}

async fn respond(
    State(state): State<StubState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    state.requests.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path: uri.path().to_string(),
        authorization: headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        body: String::from_utf8_lossy(&body).into_owned(),
    });

    match state
        .routes
        .get(&(method.to_string(), uri.path().to_string()))
    {
        Some((status, body)) => {
            let status = StatusCode::from_u16(*status).unwrap();
            if body.is_empty() {
                status.into_response()
            } else {
                (
                    status,
                    [(header::CONTENT_TYPE, "application/json")],
                    body.clone(),
                )
                    .into_response()
            }
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A server address that refuses connections immediately.
pub const UNREACHABLE_URL: &str = "http://127.0.0.1:1";
