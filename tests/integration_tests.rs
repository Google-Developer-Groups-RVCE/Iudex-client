use std::fs;
use tempfile::TempDir;

use cp_client::api::models::TestInput;
use cp_client::config::Config;
use cp_client::judge::engine::JudgeEngine;
use cp_client::judge::result::LocalExecutionStatus;
use cp_client::languages::{Invocation, Language};

#[tokio::test]
async fn test_python_local_run_success() {
    let config = Config::default();
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("solution.py");

    fs::write(
        &source_path,
        "import sys\na, b = map(int, sys.stdin.read().split())\nprint(a + b)\n",
    )
    .unwrap();

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
        assert_eq!(
            res.status,
            LocalExecutionStatus::Success,
            "memory_mb={:?}",
            memory_mb
        );
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

// ---------------------------------------------------------------------------
// Memory-limit plumbing
//
// Regression tests for the half-landed change that broke the build: the runner
// was written to call `Language::execute(.., memory_mb)` and
// `Language::limit_address_space()` before either existed on `Language`. The
// build failure was the visible symptom; the contract below is the thing that
// was actually missing, so it is pinned here rather than left implicit.
// ---------------------------------------------------------------------------

/// The `Invocation` a language would be run with, for a given memory cap.
fn invocation_for(language: Language, memory_mb: u64) -> (Invocation, TempDir) {
    let config = Config::default();
    let work_dir = TempDir::new().unwrap();
    let invocation = language.execute(&config, work_dir.path(), memory_mb);
    // The workspace is returned so it outlives the paths inside `invocation`.
    (invocation, work_dir)
}

#[test]
fn java_is_capped_with_xmx_when_a_limit_is_set() {
    let (inv, _dir) = invocation_for(Language::Java, 512);
    assert!(
        inv.args.iter().any(|a| a == "-Xmx512m"),
        "expected -Xmx512m in {:?}",
        inv.args
    );
}

#[test]
fn java_is_left_uncapped_when_the_limit_is_zero() {
    // `0` means "no limit" everywhere else in the judge; it must not become
    // `-Xmx0m`, which the JVM rejects outright.
    let (inv, _dir) = invocation_for(Language::Java, 0);
    assert!(
        !inv.args.iter().any(|a| a.starts_with("-Xmx")),
        "expected no heap flag in {:?}",
        inv.args
    );
}

#[test]
fn jvm_options_precede_the_class_name() {
    // Anything after the class name is passed to the submitted program instead
    // of the JVM, so the flag would be silently ignored and the limit lost.
    let (inv, _dir) = invocation_for(Language::Java, 256);

    let xmx = inv.args.iter().position(|a| a.starts_with("-Xmx"));
    let class = inv.args.iter().position(|a| a == "solution");

    assert!(xmx.is_some(), "no heap flag in {:?}", inv.args);
    assert!(class.is_some(), "no class name in {:?}", inv.args);
    assert!(
        xmx < class,
        "-Xmx must come before the class name, got {:?}",
        inv.args
    );
}

#[test]
fn the_memory_limit_does_not_leak_into_other_languages() {
    // C++ and Python are capped by the OS, not on their command line. A stray
    // argument here would be handed to the submitted program as input.
    for language in [Language::Cpp, Language::Python] {
        let (unlimited, _a) = invocation_for(language, 0);
        let (capped, _b) = invocation_for(language, 512);
        assert_eq!(
            unlimited.args.len(),
            capped.args.len(),
            "{} gained an argument from the memory limit: {:?}",
            language.name(),
            capped.args
        );
    }
}

#[test]
fn the_os_level_cap_is_skipped_only_for_java() {
    // `RLIMIT_AS` caps virtual address space, not resident memory. A JVM
    // reserves far more address space than it commits, so applying it stops
    // `java` from starting rather than bounding its heap.
    assert!(!Language::Java.limit_address_space());
    assert!(Language::Cpp.limit_address_space());
    assert!(Language::Python.limit_address_space());
}

#[test]
fn every_language_is_capped_by_exactly_one_mechanism() {
    // The invariant the original change was reaching for: a language is bounded
    // by the OS or by its own runtime, never by both and never by neither.
    // Neither is a judge that silently ignores the limit; both is a JVM that
    // will not start. This is the assertion that would have caught the gap.
    for language in [Language::Cpp, Language::Java, Language::Python] {
        let (inv, _dir) = invocation_for(language, 512);
        let self_capped = inv.args.iter().any(|a| a.starts_with("-Xmx"));
        let os_capped = language.limit_address_space();

        assert_ne!(
            os_capped,
            self_capped,
            "{} is capped by {}",
            language.name(),
            if os_capped {
                "both mechanisms"
            } else {
                "neither mechanism"
            }
        );
    }
}

/// End-to-end proof that a Java submission still runs with a limit applied.
/// Skipped where no JVM is installed, so the suite stays green on machines that
/// only judge C++ and Python.
#[tokio::test]
async fn java_submission_runs_under_a_memory_limit() {
    // macOS ships a `javac` shim that runs fine and exits non-zero when no JDK
    // is installed, so the spawn succeeding is not enough - check the status.
    let have_jvm = std::process::Command::new("javac")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success());
    if !have_jvm {
        eprintln!("skipping: no JVM on this machine");
        return;
    }

    let config = Config::default();
    let source_code = r#"
import java.util.Scanner;
public class solution {
    public static void main(String[] args) {
        Scanner sc = new Scanner(System.in);
        System.out.println(sc.nextInt() + sc.nextInt());
    }
}
"#;

    let test_inputs = vec![TestInput {
        id: "tc_1".to_string(),
        input_data: "5 7\n".to_string(),
    }];

    let sub_res = JudgeEngine::execute_submission(
        &config,
        "A",
        Language::Java,
        source_code,
        &test_inputs,
        Some(10_000),
        Some(512),
    )
    .await
    .unwrap();

    assert!(
        sub_res.compilation.as_ref().unwrap().success,
        "javac failed: {}",
        sub_res.compilation.as_ref().unwrap().stderr
    );
    assert_eq!(
        sub_res.test_results[0].status,
        LocalExecutionStatus::Success,
        "stderr: {}",
        sub_res.test_results[0].stderr
    );
    assert_eq!(sub_res.test_results[0].stdout.trim(), "12");
}
