use tempfile::TempDir;
use std::fs;

use cp_client::config::Config;
use cp_client::judge::engine::JudgeEngine;
use cp_client::judge::result::LocalExecutionStatus;
use cp_client::languages::Language;
use cp_client::api::models::TestInput;

#[tokio::test]
async fn test_python_local_run_success() {
    let config = Config::default();
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");

    fs::write(&source_path, "import sys\na, b = map(int, sys.stdin.read().split())\nprint(a + b)\n").unwrap();

    let res = JudgeEngine::run_local_single(
        &config,
        Language::Python,
        &source_path,
        "10 20\n",
        Some(2000),
    )
    .await
    .unwrap();

    assert_eq!(res.status, LocalExecutionStatus::Success);
    assert_eq!(res.stdout.trim(), "30");
}

#[tokio::test]
async fn test_python_timeout() {
    let config = Config::default();
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");

    fs::write(&source_path, "import time\ntime.sleep(5)\nprint('done')\n").unwrap();

    let res = JudgeEngine::run_local_single(
        &config,
        Language::Python,
        &source_path,
        "",
        Some(500), // 500ms timeout
    )
    .await
    .unwrap();

    assert_eq!(res.status, LocalExecutionStatus::TimeLimitExceeded);
}

#[tokio::test]
async fn test_cpp_submission_execution() {
    let config = Config::default();
    let source_code = r#"
#include <iostream>
using namespace std;
int main() {
    int a, b;
    if (cin >> a >> b) {
        cout << (a + b) << endl;
    }
    return 0;
}
"#;

    let test_inputs = vec![
        TestInput {
            id: "tc_1".to_string(),
            input_data: "5 7\n".to_string(),
        },
        TestInput {
            id: "tc_2".to_string(),
            input_data: "100 200\n".to_string(),
        },
    ];

    let sub_res = JudgeEngine::execute_submission(
        &config,
        "A",
        Language::Cpp,
        source_code,
        &test_inputs,
        Some(3000),
    )
    .await
    .unwrap();

    assert!(sub_res.compilation.as_ref().unwrap().success);
    assert_eq!(sub_res.test_results.len(), 2);
    assert_eq!(sub_res.test_results[0].stdout.trim(), "12");
    assert_eq!(sub_res.test_results[1].stdout.trim(), "300");
}

#[tokio::test]
async fn test_cpp_compilation_failure() {
    let config = Config::default();
    let invalid_cpp = "int main() { invalid_syntax_here; }";

    let test_inputs = vec![TestInput {
        id: "tc_1".to_string(),
        input_data: "1 2\n".to_string(),
    }];

    let sub_res = JudgeEngine::execute_submission(
        &config,
        "A",
        Language::Cpp,
        invalid_cpp,
        &test_inputs,
        Some(3000),
    )
    .await
    .unwrap();

    let comp = sub_res.compilation.as_ref().unwrap();
    assert!(!comp.success);
    assert!(sub_res.test_results.is_empty());
}
