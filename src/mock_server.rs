use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

use crate::api::models::{
    AuthoritativeVerdict, Contest, LoginRequest, LoginResponse, Problem, ProblemSummary, SubmissionPayload,
    TestInput, VerdictStatus,
};
use crate::judge::result::LocalExecutionStatus;

struct MockProblemState {
    problem: Problem,
    test_inputs: Vec<TestInput>,
    expected_outputs: HashMap<String, String>,
}

#[derive(Clone)]
struct AppState {
    problems: Arc<HashMap<String, MockProblemState>>,
    contests: Arc<Vec<Contest>>,
}

impl AppState {
    fn new() -> Self {
        let mut problems = HashMap::new();

        // Problem A: Addition
        let mut prob_a_expected = HashMap::new();
        prob_a_expected.insert("tc_1".to_string(), "8".to_string());
        prob_a_expected.insert("tc_2".to_string(), "15".to_string());
        prob_a_expected.insert("tc_3".to_string(), "300".to_string());

        problems.insert(
            "A".to_string(),
            MockProblemState {
                problem: Problem {
                    id: "A".to_string(),
                    contest_id: "contest-101".to_string(),
                    title: "A. Simple Addition".to_string(),
                    statement: "Given two space-separated integers A and B, output their sum.".to_string(),
                    input_spec: "Two space-separated integers A and B.".to_string(),
                    output_spec: "Print a single integer A + B.".to_string(),
                    constraints: "-10^9 <= A, B <= 10^9".to_string(),
                    time_limit_ms: 2000,
                    memory_limit_mb: 256,
                },
                test_inputs: vec![
                    TestInput {
                        id: "tc_1".to_string(),
                        input_data: "3 5\n".to_string(),
                    },
                    TestInput {
                        id: "tc_2".to_string(),
                        input_data: "-10 25\n".to_string(),
                    },
                    TestInput {
                        id: "tc_3".to_string(),
                        input_data: "100 200\n".to_string(),
                    },
                ],
                expected_outputs: prob_a_expected,
            },
        );

        // Problem B: Palindrome
        let mut prob_b_expected = HashMap::new();
        prob_b_expected.insert("tc_1".to_string(), "YES".to_string());
        prob_b_expected.insert("tc_2".to_string(), "NO".to_string());

        problems.insert(
            "B".to_string(),
            MockProblemState {
                problem: Problem {
                    id: "B".to_string(),
                    contest_id: "contest-101".to_string(),
                    title: "B. Palindrome Verification".to_string(),
                    statement: "Check if a given string is a palindrome. Output YES or NO.".to_string(),
                    input_spec: "A single string S.".to_string(),
                    output_spec: "YES or NO".to_string(),
                    constraints: "1 <= |S| <= 100".to_string(),
                    time_limit_ms: 1000,
                    memory_limit_mb: 128,
                },
                test_inputs: vec![
                    TestInput {
                        id: "tc_1".to_string(),
                        input_data: "racecar\n".to_string(),
                    },
                    TestInput {
                        id: "tc_2".to_string(),
                        input_data: "hello\n".to_string(),
                    },
                ],
                expected_outputs: prob_b_expected,
            },
        );

        let contests = vec![Contest {
            id: "contest-101".to_string(),
            title: "Mock Practice Round 1".to_string(),
            description: "Practice round for client judging system verification.".to_string(),
            start_time: "2026-08-08T00:00:00Z".to_string(),
            end_time: "2026-08-08T23:59:59Z".to_string(),
            problems: vec![
                ProblemSummary {
                    id: "A".to_string(),
                    title: "A. Simple Addition".to_string(),
                    score: 100,
                },
                ProblemSummary {
                    id: "B".to_string(),
                    title: "B. Palindrome Verification".to_string(),
                    score: 200,
                },
            ],
        }];

        Self {
            problems: Arc::new(problems),
            contests: Arc::new(contests),
        }
    }
}

pub async fn run_mock_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState::new();

    let app = Router::new()
        .route("/api/auth/login", post(handle_login))
        .route("/api/contests", get(handle_get_contests))
        .route("/api/contests/:id", get(handle_get_contest))
        .route("/api/problems/:id", get(handle_get_problem))
        .route("/api/problems/:id/tests", get(handle_get_test_inputs))
        .route("/api/submissions", post(handle_submit_result))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Starting Mock Contest Server on http://{}", addr);
    println!("🚀 Mock Contest Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_login(Json(req): Json<LoginRequest>) -> (StatusCode, Json<LoginResponse>) {
    info!("Login request for user: {}", req.username);
    let resp = LoginResponse {
        token: format!("mock_token_for_{}", req.username),
        username: req.username,
    };
    (StatusCode::OK, Json(resp))
}

async fn handle_get_contests(State(state): State<AppState>) -> Json<Vec<Contest>> {
    Json((*state.contests).clone())
}

async fn handle_get_contest(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Contest>, StatusCode> {
    state
        .contests
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn handle_get_problem(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Problem>, StatusCode> {
    state
        .problems
        .get(&id)
        .map(|p| Json(p.problem.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn handle_get_test_inputs(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TestInput>>, StatusCode> {
    state
        .problems
        .get(&id)
        .map(|p| Json(p.test_inputs.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn handle_submit_result(
    State(state): State<AppState>,
    Json(payload): Json<SubmissionPayload>,
) -> Result<Json<AuthoritativeVerdict>, StatusCode> {
    info!(
        "Received submission result for problem '{}' in language '{}'",
        payload.problem_id, payload.language
    );

    let prob_state = state
        .problems
        .get(&payload.problem_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let submission_res = &payload.result;

    // 1. Check compilation
    if let Some(comp) = &submission_res.compilation {
        if !comp.success {
            return Ok(Json(AuthoritativeVerdict {
                submission_id: format!("sub_{}", rand_id()),
                verdict: VerdictStatus::CompilationError,
                passed_test_cases: 0,
                total_test_cases: prob_state.test_inputs.len(),
                message: format!("Compilation Error:\n{}", comp.stderr),
            }));
        }
    }

    let total = prob_state.test_inputs.len();
    let mut passed = 0;
    let mut final_verdict = VerdictStatus::Accepted;
    let mut detail_message = String::new();

    // 2. Verify candidate test execution outputs against server's authoritative expected output
    for test_res in &submission_res.test_results {
        match &test_res.status {
            LocalExecutionStatus::TimeLimitExceeded => {
                final_verdict = VerdictStatus::TimeLimitExceeded;
                detail_message = format!("Time Limit Exceeded on test case {}", test_res.test_case_id);
                break;
            }
            LocalExecutionStatus::OutputLimitExceeded => {
                final_verdict = VerdictStatus::MemoryLimitExceeded;
                detail_message = format!("Output Limit Exceeded on test case {}", test_res.test_case_id);
                break;
            }
            LocalExecutionStatus::RuntimeError { exit_code } => {
                final_verdict = VerdictStatus::RuntimeError;
                detail_message = format!(
                    "Runtime Error (code {:?}) on test case {}",
                    exit_code, test_res.test_case_id
                );
                break;
            }
            LocalExecutionStatus::Success => {
                let expected = prob_state
                    .expected_outputs
                    .get(&test_res.test_case_id)
                    .map(|s| s.trim())
                    .unwrap_or("");
                let actual = test_res.stdout.trim();

                if actual == expected {
                    passed += 1;
                } else {
                    final_verdict = VerdictStatus::WrongAnswer;
                    detail_message = format!(
                        "Wrong Answer on test case {}. (Authoritative verification failed)",
                        test_res.test_case_id
                    );
                    break;
                }
            }
        }
    }

    if final_verdict == VerdictStatus::Accepted {
        detail_message = format!("All {} test cases passed!", total);
    }

    Ok(Json(AuthoritativeVerdict {
        submission_id: format!("sub_{}", rand_id()),
        verdict: final_verdict,
        passed_test_cases: passed,
        total_test_cases: total,
        message: detail_message,
    }))
}

fn rand_id() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(12345)
}
