use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;
#[cfg(not(target_os = "linux"))]
use tracing::warn;

use crate::config::Config;
use crate::error::Result;
use crate::judge::result::{LocalExecutionStatus, TestResult};
use crate::languages::{Invocation, Language};

/// Resource caps applied to a single judged process.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout_ms: u64,
    /// Address-space cap in MB; `0` disables enforcement.
    pub memory_mb: u64,
}

pub struct ProcessRunner;

impl ProcessRunner {
    pub async fn run_test_case(
        config: &Config,
        language: Language,
        work_dir: &Path,
        test_case_id: &str,
        input: &str,
        limits: Limits,
    ) -> Result<TestResult> {
        let invocation = language.execute(config, work_dir);

        let mut child = build_command(&invocation, work_dir, language, limits.memory_mb)
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
        let timeout_duration = Duration::from_millis(limits.timeout_ms);

        let stdout_pipe = child.stdout.take().expect("stdout is piped");
        let stderr_pipe = child.stderr.take().expect("stderr is piped");
        let limit = config.max_output_bytes;

        // Each pipe is drained by its own task. They must run concurrently: a
        // child that fills one pipe's buffer (~64KB) blocks on write and never
        // closes the other, so reading them in sequence deadlocks until the
        // timeout fires.
        let mut stdout_task = tokio::spawn(read_capped(stdout_pipe, limit));
        let mut stderr_task = tokio::spawn(read_capped(stderr_pipe, limit));

        let execution_future = async {
            let mut stdout_done: Option<(Vec<u8>, bool)> = None;
            let mut stderr_done: Option<(Vec<u8>, bool)> = None;

            while stdout_done.is_none() || stderr_done.is_none() {
                tokio::select! {
                    res = &mut stdout_task, if stdout_done.is_none() => {
                        stdout_done = Some(res.expect("stdout reader panicked")?);
                    }
                    res = &mut stderr_task, if stderr_done.is_none() => {
                        stderr_done = Some(res.expect("stderr reader panicked")?);
                    }
                }

                let hit_cap = stdout_done.as_ref().is_some_and(|(_, over)| *over)
                    || stderr_done.as_ref().is_some_and(|(_, over)| *over);

                // Stop waiting on the other stream the moment the cap is blown.
                // Nothing is draining the pipes any more, so the program would
                // block on its next write and only surface at the timeout - as
                // a TLE rather than the output-limit breach it actually is.
                if hit_cap {
                    let _ = child.start_kill();
                    break;
                }
            }

            // The kill closes the abandoned pipe, so its task finishes promptly.
            let (stdout_bytes, stdout_exceeded) = match stdout_done {
                Some(done) => done,
                None => stdout_task.await.expect("stdout reader panicked")?,
            };
            let (stderr_bytes, stderr_exceeded) = match stderr_done {
                Some(done) => done,
                None => stderr_task.await.expect("stderr reader panicked")?,
            };

            let status = child.wait().await?;
            Ok::<_, std::io::Error>((
                stdout_bytes,
                stderr_bytes,
                status,
                stdout_exceeded || stderr_exceeded,
            ))
        };

        // Bound the future to a `let` so its borrow of `child` ends before the
        // timeout branch below needs the child back.
        let outcome = timeout(timeout_duration, execution_future).await;

        match outcome {
            Ok(Ok((stdout_raw, stderr_raw, status, output_exceeded))) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let exit_code = status.code();

                let stdout = String::from_utf8_lossy(&stdout_raw).to_string();
                let stderr = String::from_utf8_lossy(&stderr_raw).to_string();

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

/// Reads at most `limit` bytes, reporting whether the stream had more.
///
/// The cap has to be applied while reading: buffering the whole stream and
/// truncating afterwards lets a runaway program consume unbounded memory before
/// the limit is ever consulted, which is what the limit exists to prevent.
async fn read_capped<R>(reader: R, limit: usize) -> std::io::Result<(Vec<u8>, bool)>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut buffer = Vec::new();
    // One byte past the cap, so landing exactly on it is distinguishable from
    // overshooting it.
    tokio::io::copy(
        &mut tokio::io::AsyncReadExt::take(reader, limit as u64 + 1),
        &mut buffer,
    )
    .await?;

    let exceeded = buffer.len() > limit;
    buffer.truncate(limit);
    Ok((buffer, exceeded))
}

fn build_command(
    invocation: &Invocation,
    work_dir: &Path,
    language: Language,
    memory_mb: u64,
) -> Command {
    let mut std_command = std::process::Command::new(&invocation.program);
    std_command.args(&invocation.args).current_dir(work_dir);

    if memory_mb > 0 && language.limit_address_space() {
        apply_address_space_limit(&mut std_command, memory_mb);
    }

    Command::from(std_command)
}

/// Caps the child's address space via `RLIMIT_AS`, applied between fork and
/// exec so the judged program can never raise it back.
#[cfg(target_os = "linux")]
fn apply_address_space_limit(command: &mut std::process::Command, memory_mb: u64) {
    use std::os::unix::process::CommandExt;

    let bytes = memory_mb.saturating_mul(1024 * 1024);

    // SAFETY: only `setrlimit` runs in the forked child, and it is
    // async-signal-safe, which is the requirement for a `pre_exec` hook.
    unsafe {
        command.pre_exec(move || {
            let limit = libc::rlimit {
                rlim_cur: bytes as libc::rlim_t,
                rlim_max: bytes as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Darwin aliases `RLIMIT_AS` onto `RLIMIT_RSS` and rejects any attempt to
/// lower it (`EINVAL`, even against an infinite hard limit), and Windows has no
/// equivalent here. Rather than silently ignore the request, say so: an
/// unenforced limit on a judge is worse than a stated one.
#[cfg(not(target_os = "linux"))]
fn apply_address_space_limit(_command: &mut std::process::Command, memory_mb: u64) {
    // Once per process: a submission runs this for every test case, and the
    // situation is a property of the platform, not of any one run.
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        warn!(
            "Memory limit of {}MB not applied: OS-level address-space limits are only \
             supported on Linux. Java submissions are still capped via -Xmx.",
            memory_mb
        );
    });
}
