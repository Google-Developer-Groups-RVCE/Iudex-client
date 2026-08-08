use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalExecutionStatus {
    Success,
    RuntimeError { exit_code: Option<i32> },
    TimeLimitExceeded,
    OutputLimitExceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_case_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub status: LocalExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTelemetry {
    pub os: String,
    pub architecture: String,
    pub compiler: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionResult {
    pub problem_id: String,
    pub language: String,
    pub compilation: Option<CompilationResult>,
    pub test_results: Vec<TestResult>,
    pub telemetry: ExecutionTelemetry,
}
