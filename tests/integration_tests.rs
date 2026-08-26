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
        None,
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
        None,
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
        None,
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
        None,
    )
    .await
    .unwrap();

    let comp = sub_res.compilation.as_ref().unwrap();
    assert!(!comp.success);
    assert!(sub_res.test_results.is_empty());
}

/// Regression test for a deadlock in the process runner: it used to drain
/// stdout to EOF before reading stderr, so a child that filled the stderr pipe
/// buffer blocked forever and was misreported as a timeout.
#[tokio::test]
async fn test_large_stderr_does_not_deadlock() {
    let config = Config::default();
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");

    fs::write(
        &source_path,
        "import sys\nsys.stderr.write('x' * 1_000_000)\nprint('ok')\n",
    )
    .unwrap();

    let res = JudgeEngine::run_local_single(
        &config,
        Language::Python,
        &source_path,
        "",
        Some(5000),
        None,
    )
    .await
    .unwrap();

    assert_eq!(res.status, LocalExecutionStatus::Success);
    assert_eq!(res.stdout.trim(), "ok");
    assert_eq!(res.stderr.len(), 1_000_000);
}

/// A memory cap of 0 must leave the process unconstrained, and a generous cap
/// must not break an interpreter that reserves address space at startup.
#[tokio::test]
async fn test_memory_limit_allows_normal_programs() {
    let config = Config::default();
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");

    fs::write(&source_path, "print(sum(range(1000)))\n").unwrap();

    for memory_mb in [None, Some(0), Some(2048)] {
        let res = JudgeEngine::run_local_single(
            &config,
            Language::Python,
            &source_path,
            "",
            Some(5000),
            memory_mb,
        )
        .await
        .unwrap();
        assert_eq!(res.status, LocalExecutionStatus::Success, "memory_mb={:?}", memory_mb);
        assert_eq!(res.stdout.trim(), "499500");
    }
}

/// The output cap must stop the program at the cap, not buffer the whole
/// stream and truncate afterwards. A runaway printer previously consumed
/// unbounded memory until the timeout fired and was reported as a TLE.
#[tokio::test]
async fn test_runaway_output_is_capped_and_stopped() {
    let config = Config {
        max_output_bytes: 64 * 1024,
        ..Config::default()
    };

    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");
    fs::write(
        &source_path,
        "import sys\nw = sys.stdout.write\nblock = 'y' * 65536\nwhile True:\n    w(block)\n",
    )
    .unwrap();

    let started = std::time::Instant::now();
    let res = JudgeEngine::run_local_single(
        &config,
        Language::Python,
        &source_path,
        "",
        Some(10_000),
        None,
    )
    .await
    .unwrap();

    assert_eq!(res.status, LocalExecutionStatus::OutputLimitExceeded);
    assert_eq!(res.stdout.len(), config.max_output_bytes);
    // Killed at the cap rather than left running until the 10s timeout.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}, so the program was not stopped at the cap",
        started.elapsed()
    );
}

/// Output that fits under the cap must be returned intact and reported Success.
#[tokio::test]
async fn test_output_just_under_cap_is_not_flagged() {
    let config = Config {
        max_output_bytes: 64 * 1024,
        ..Config::default()
    };

    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");
    fs::write(&source_path, "import sys\nsys.stdout.write('z' * 65535)\n").unwrap();

    let res = JudgeEngine::run_local_single(
        &config,
        Language::Python,
        &source_path,
        "",
        Some(5000),
        None,
    )
    .await
    .unwrap();

    assert_eq!(res.status, LocalExecutionStatus::Success);
    assert_eq!(res.stdout.len(), 65535);
}
