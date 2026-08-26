use serde::{Deserialize, Serialize};
use crate::judge::result::SubmissionResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
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
