use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client as HttpClient;

use crate::api::models::{
    AuthoritativeVerdict, Contest, LoginRequest, LoginResponse, Problem, SubmissionPayload,
    TestInput,
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

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let url = format!("{}/api/auth/login", self.server_url);
        let req = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };

        let res = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| ClientError::Network(format!("Login connection error: {}", e)))?;

        if !res.status().is_success() {
            let status = res.status().as_u16();
            let text = res.text().await.unwrap_or_default();
            return Err(ClientError::ApiError {
                message: format!("Authentication failed: {}", text),
                status_code: Some(status),
            });
        }

        let login_res: LoginResponse = res.json().await?;
        Ok(login_res)
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

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        res: reqwest::Response,
    ) -> Result<T> {
        let status = res.status();
        if status.is_success() {
            let data: T = res.json().await?;
            Ok(data)
        } else {
            let status_code = status.as_u16();
            let text = res.text().await.unwrap_or_default();
            Err(ClientError::ApiError {
                message: text,
                status_code: Some(status_code),
            })
        }
    }
}
