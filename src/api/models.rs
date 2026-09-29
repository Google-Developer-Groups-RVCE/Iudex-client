use crate::judge::result::SubmissionResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Registration takes the same fields as login. The server additionally
/// requires a 3-50 character username and an 8-72 character password.
pub type RegisterRequest = LoginRequest;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Role {
    Admin,
    Contestmaster,
    Contestant,
    /// A role added on the server after this client was built. Kept so that a
    /// new role cannot make login fail to deserialize.
    #[serde(other)]
    Unknown,
}

/// Returned by both `POST /auth/login` and `POST /auth/register`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub username: String,
    pub role: Role,
}

/// Returned by `GET /auth/me`. The server sends `role` as null for an account
/// with no authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
    pub username: String,
    pub role: Option<Role>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemSummary {
    pub id: String,
    pub title: String,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start_time: String,
    pub end_time: String,
    pub problems: Vec<ProblemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: String,
    pub contest_id: String,
    pub title: String,
    pub statement: String,
    pub input_spec: String,
    pub output_spec: String,
    pub constraints: String,
    pub time_limit_ms: u64,
    pub memory_limit_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInput {
    pub id: String,
    pub input_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionPayload {
    pub problem_id: String,
    pub language: String,
    pub source_code: String,
    pub result: SubmissionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerdictStatus {
    Accepted,
    WrongAnswer,
    CompilationError,
    RuntimeError,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    OutputLimitExceeded,
    SystemError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthoritativeVerdict {
    pub submission_id: String,
    pub verdict: VerdictStatus,
    pub passed_test_cases: usize,
    pub total_test_cases: usize,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::{AuthResponse, MeResponse, Role};

    #[test]
    fn roles_use_the_servers_uppercase_names() {
        for (wire, role) in [
            ("\"ADMIN\"", Role::Admin),
            ("\"CONTESTMASTER\"", Role::Contestmaster),
            ("\"CONTESTANT\"", Role::Contestant),
        ] {
            assert_eq!(serde_json::from_str::<Role>(wire).unwrap(), role);
            assert_eq!(serde_json::to_string(&role).unwrap(), wire);
        }
    }

    #[test]
    fn an_unrecognised_role_does_not_fail_parsing() {
        assert_eq!(
            serde_json::from_str::<Role>("\"JUDGE\"").unwrap(),
            Role::Unknown
        );
    }

    #[test]
    fn auth_response_matches_the_server_shape() {
        let parsed: AuthResponse =
            serde_json::from_str(r#"{"token":"t","username":"alice","role":"ADMIN"}"#).unwrap();
        assert_eq!(parsed.token, "t");
        assert_eq!(parsed.username, "alice");
        assert_eq!(parsed.role, Role::Admin);
    }

    #[test]
    fn me_response_allows_a_null_or_missing_role() {
        let null: MeResponse = serde_json::from_str(r#"{"username":"a","role":null}"#).unwrap();
        assert_eq!(null.role, None);
        let missing: MeResponse = serde_json::from_str(r#"{"username":"a"}"#).unwrap();
        assert_eq!(missing.role, None);
    }
}
