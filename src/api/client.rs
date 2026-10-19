use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client as HttpClient, StatusCode};

use crate::api::models::{
    AuthResponse, AuthoritativeVerdict, Contest, LoginRequest, MeResponse, Problem,
    RegisterRequest, SubmissionPayload, TestInput,
};
use crate::config::Config;
use crate::error::{ClientError, Result};

pub struct ApiClient {
    http: HttpClient,
    server_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(config: &Config) -> Self {
        Self {
            http: HttpClient::new(),
            server_url: config.server_url.trim_end_matches('/').to_string(),
            token: config.auth_token.clone(),
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.token {
            let auth_val = format!("Bearer {}", token);
            if let Ok(val) = HeaderValue::from_str(&auth_val) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    /// The auth routes live at `/auth/*` on the server, outside the `/api`
    /// prefix every other route uses.
    pub async fn login(&self, username: &str, password: &str) -> Result<AuthResponse> {
        let req = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        self.post_credentials("/auth/login", &req, "Login").await
    }

    /// Creates a contestant account. The server signs the new user in, so the
    /// response carries a token just like login.
    pub async fn register(&self, username: &str, password: &str) -> Result<AuthResponse> {
        let req = RegisterRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        self.post_credentials("/auth/register", &req, "Registration")
            .await
    }

    /// Asks the server who the saved token belongs to, which also proves the
    /// token is still accepted.
    pub async fn me(&self) -> Result<MeResponse> {
        let url = format!("{}/auth/me", self.server_url);
        let res = self.http.get(&url).headers(self.headers()).send().await?;

        self.handle_response(res).await
    }

    /// Login and registration send no token, so a 401 here means rejected
    /// credentials, not an expired session, and is reported as the server
    /// phrased it rather than through `handle_response`.
    async fn post_credentials(
        &self,
        path: &str,
        req: &LoginRequest,
        action: &str,
    ) -> Result<AuthResponse> {
        let url = format!("{}{}", self.server_url, path);
        let res = self
            .http
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| ClientError::Network(format!("{} connection error: {}", action, e)))?;

        let status = res.status();
        if !status.is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ClientError::ApiError {
                message: server_error_message(status, &text),
                status_code: Some(status.as_u16()),
            });
        }

        Ok(res.json().await?)
    }

    pub async fn get_contests(&self) -> Result<Vec<Contest>> {
        let url = format!("{}/api/contests", self.server_url);
        let res = self.http.get(&url).headers(self.headers()).send().await?;

        self.handle_response(res).await
    }

    pub async fn get_contest(&self, contest_id: &str) -> Result<Contest> {
        let url = format!("{}/api/contests/{}", self.server_url, contest_id);
        let res = self.http.get(&url).headers(self.headers()).send().await?;

        self.handle_response(res).await
    }

    pub async fn get_problem(&self, problem_id: &str) -> Result<Problem> {
        let url = format!("{}/api/problems/{}", self.server_url, problem_id);
        let res = self.http.get(&url).headers(self.headers()).send().await?;

        self.handle_response(res).await
    }

    pub async fn get_test_inputs(&self, problem_id: &str) -> Result<Vec<TestInput>> {
        let url = format!("{}/api/problems/{}/tests", self.server_url, problem_id);
        let res = self.http.get(&url).headers(self.headers()).send().await?;

        self.handle_response(res).await
    }

    pub async fn submit_result(&self, payload: &SubmissionPayload) -> Result<AuthoritativeVerdict> {
        let url = format!("{}/api/submissions", self.server_url);
        let res = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(payload)
            .send()
            .await?;

        self.handle_response(res).await
    }

    /// Revokes the saved token on the server. The server answers `204` and, as
    /// a side effect, invalidates every other token for the same account.
    pub async fn logout(&self) -> Result<()> {
        let url = format!("{}/auth/logout", self.server_url);
        let res = self.http.post(&url).headers(self.headers()).send().await?;

        self.check_status(res).await.map(|_| ())
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        res: reqwest::Response,
    ) -> Result<T> {
        let res = self.check_status(res).await?;
        Ok(res.json().await?)
    }

    /// Passes a successful response through, and turns anything else into the
    /// matching error. Separate from `handle_response` for bodiless replies.
    async fn check_status(&self, res: reqwest::Response) -> Result<reqwest::Response> {
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }

        // The server answers a missing, expired or unverifiable token with a
        // bare 401 and no body, so the useful message has to come from here.
        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized(self.unauthorized_message()));
        }

        let text = res.text().await.unwrap_or_default();
        Err(ClientError::ApiError {
            message: server_error_message(status, &text),
            status_code: Some(status.as_u16()),
        })
    }

    fn unauthorized_message(&self) -> String {
        if self.token.is_none() {
            "You are not logged in. Run `cp-client login` first.".to_string()
        } else {
            "The server rejected your saved session. It may have expired, been ended by a \
             logout on another device, or the server may have restarted with a new signing \
             key. Run `cp-client login` again."
                .to_string()
        }
    }
}

/// Turns the server's error body into a readable line.
///
/// The server reports failures as `{"error": "..."}`, and validation failures
/// add `"fields": {"name": "reason"}`. Anything else (a proxy's HTML page, an
/// empty body) is passed through, or replaced by the status text when empty.
fn server_error_message(status: StatusCode, body: &str) -> String {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        let body = body.trim();
        return if body.is_empty() {
            format!("HTTP {}", status)
        } else {
            body.to_string()
        };
    };

    let mut message = json
        .get("error")
        .and_then(|e| e.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("HTTP {}", status));

    if let Some(fields) = json.get("fields").and_then(|f| f.as_object()) {
        let details: Vec<String> = fields
            .iter()
            .map(|(field, reason)| {
                let reason = reason
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| reason.to_string());
                format!("{}: {}", field, reason)
            })
            .collect();
        if !details.is_empty() {
            message = format!("{} ({})", message, details.join("; "));
        }
    }

    message
}

#[cfg(test)]
mod tests {
    use super::server_error_message;
    use reqwest::StatusCode;

    #[test]
    fn extracts_the_error_field() {
        let body = r#"{"error":"Invalid username or password"}"#;
        assert_eq!(
            server_error_message(StatusCode::UNAUTHORIZED, body),
            "Invalid username or password"
        );
    }

    #[test]
    fn lists_validation_failures_by_field() {
        let body = r#"{"error":"Validation failed","fields":{"password":"password must be between 8 and 72 characters"}}"#;
        assert_eq!(
            server_error_message(StatusCode::BAD_REQUEST, body),
            "Validation failed (password: password must be between 8 and 72 characters)"
        );
    }

    #[test]
    fn falls_back_to_the_status_for_an_empty_body() {
        assert_eq!(
            server_error_message(StatusCode::FORBIDDEN, ""),
            "HTTP 403 Forbidden"
        );
    }

    #[test]
    fn passes_through_a_non_json_body() {
        assert_eq!(
            server_error_message(StatusCode::BAD_GATEWAY, "upstream down"),
            "upstream down"
        );
    }
}
