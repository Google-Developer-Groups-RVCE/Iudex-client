use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::Result;
use crate::judge::result::{LocalExecutionStatus, TestResult};
use crate::languages::Language;

pub struct ProcessRunner;

impl ProcessRunner {
    pub async fn run_test_case(
        config: &Config,
        language: Language,
        work_dir: &Path,
        test_case_id: &str,
        input: &str,
        timeout_ms: u64,
    ) -> Result<TestResult> {
        let invocation = language.execute(config, work_dir);

        let mut child = Command::new(&invocation.program)
            .args(&invocation.args)
            .current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let input_bytes = input.as_bytes().to_vec();

        tokio::spawn(async move {
            let _ = stdin.write_all(&input_bytes).await;
            let _ = stdin.flush().await;
        });

        let start = Instant::now();
        let timeout_duration = Duration::from_millis(timeout_ms);

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let execution_future = async {
            let mut stdout_bytes = Vec::new();
            let mut stderr_bytes = Vec::new();

            if let Some(ref mut out) = stdout_handle {
                let _ = tokio::io::copy(out, &mut stdout_bytes).await;
            }
            if let Some(ref mut err) = stderr_handle {
                let _ = tokio::io::copy(err, &mut stderr_bytes).await;
            }

            let status = child.wait().await?;
            Ok::<(Vec<u8>, Vec<u8>, std::process::ExitStatus), std::io::Error>((stdout_bytes, stderr_bytes, status))
        };

        match timeout(timeout_duration, execution_future).await {
            Ok(Ok((stdout_raw, stderr_raw, status))) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let exit_code = status.code();

                let mut stdout_vec = stdout_raw;
                let mut stderr_vec = stderr_raw;

                let output_exceeded = stdout_vec.len() > config.max_output_bytes
                    || stderr_vec.len() > config.max_output_bytes;

                if stdout_vec.len() > config.max_output_bytes {
                    stdout_vec.truncate(config.max_output_bytes);
                }
                if stderr_vec.len() > config.max_output_bytes {
                    stderr_vec.truncate(config.max_output_bytes);
                }

                let stdout = String::from_utf8_lossy(&stdout_vec).to_string();
                let stderr = String::from_utf8_lossy(&stderr_vec).to_string();

                let status = if output_exceeded {
                    LocalExecutionStatus::OutputLimitExceeded
                } else if !status.success() {
                    LocalExecutionStatus::RuntimeError { exit_code }
                } else {
                    LocalExecutionStatus::Success
                };

                Ok(TestResult {
                    test_case_id: test_case_id.to_string(),
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                    status,
                })
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => {
                // Process timed out - terminate child process cleanly
                let _ = child.start_kill();
                let _ = child.wait().await;
                let duration_ms = start.elapsed().as_millis() as u64;

                Ok(TestResult {
                    test_case_id: test_case_id.to_string(),
                    stdout: String::new(),
                    stderr: "Process execution timed out".to_string(),
                    exit_code: None,
                    duration_ms,
                    status: LocalExecutionStatus::TimeLimitExceeded,
                })
            }
        }
    }
}
