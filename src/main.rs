use chrono::Utc;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing_subscriber::EnvFilter;

use cp_client::api::client::ApiClient;
use cp_client::api::models::{Contest, SubmissionPayload};
use cp_client::config::Config;
use cp_client::contest::history::SubmissionRecord;
use cp_client::contest::manager::ContestManager;
use cp_client::contest::status::{contest_timing, format_duration, ContestStatus};
use cp_client::error::{Context, Result};
use cp_client::judge::engine::JudgeEngine;
use cp_client::languages::Language;
use cp_client::mock_server::run_mock_server;

#[derive(Parser, Debug)]
#[command(name = "cp-client")]
#[command(about = "Headless Rust Client Judging Core for Competitive Programming", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate with the contest server
    Login {
        /// Server base URL (e.g. http://127.0.0.1:8080)
        #[arg(short, long, default_value = "http://127.0.0.1:8080")]
        server: String,

        /// Username
        #[arg(short, long)]
        username: String,

        /// Password
        #[arg(short, long)]
        password: String,
    },

    /// List available contests
    Contests,

    /// Show details for a specific contest
    Contest {
        /// Contest ID
        id: String,
    },

    /// View problem details
    Problem {
        /// Problem ID
        id: String,
    },

    /// Run source code locally against custom test input
    Run {
        /// Source code file path
        source: PathBuf,

        /// Programming language (cpp, java, python). Auto-detected from file extension if omitted.
        #[arg(short, long)]
        lang: Option<String>,

        /// Path to input file. If omitted, reads from stdin.
        #[arg(short, long)]
        input: Option<PathBuf>,

        /// Execution timeout limit in milliseconds
        #[arg(short, long)]
        timeout: Option<u64>,

        /// Address-space limit in MB. 0 disables it.
        #[arg(short, long)]
        memory: Option<u64>,
    },

    /// Execute submission locally against problem test inputs and submit results to server
    Submit {
        /// Source code file path
        source: PathBuf,

        /// Problem ID (e.g. A, B)
        #[arg(short, long)]
        problem: String,

        /// Programming language (cpp, java, python). Auto-detected from file extension if omitted.
        #[arg(short, long)]
        lang: Option<String>,

        /// Execution timeout limit in milliseconds
        #[arg(short, long)]
        timeout: Option<u64>,

        /// Address-space limit in MB. 0 disables it.
        #[arg(short, long)]
        memory: Option<u64>,
    },

    /// List locally recorded past submissions and their verdicts
    History {
        /// Only show submissions for this problem ID
        #[arg(short, long)]
        problem: Option<String>,
    },

    /// Start a local mock contest server for development and testing
    MockServer {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::WARN.into()))
        .init();

    // Single exit path: every command reports failure by returning an error,
    // rather than each one deciding between `exit(1)` and `?`.
    if let Err(err) = run(Cli::parse()).await {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let mut config = Config::load_or_create().context("Could not load configuration")?;

    match cli.command {
        Commands::Login {
            server,
            username,
            password,
        } => {
            println!("Logging into server {} as '{}'...", server, username);
            config.server_url = server;

            let api = ApiClient::new(&config);
            let response = api
                .login(&username, &password)
                .await
                .context("Login failed")?;

            config.auth_token = Some(response.token);
            config.username = Some(response.username.clone());
            config.save().context("Could not save session")?;
            println!(
                "Login successful. Session saved for user '{}'.",
                response.username
            );
        }

        Commands::Contests => {
            let manager = ContestManager::new(&config)?;
            let contests = manager
                .list_contests()
                .await
                .context("Failed to list contests")?;

            println!("\nAvailable Contests:");
            println!("{:<15} {:<30} {:<20}", "CONTEST ID", "TITLE", "PROBLEMS");
            println!("{}", "-".repeat(68));
            for contest in contests {
                println!(
                    "{:<15} {:<30} {:<20}",
                    contest.id,
                    contest.title,
                    contest.problems.len()
                );
            }
        }

        Commands::Contest { id } => {
            let manager = ContestManager::new(&config)?;
            let contest = manager
                .fetch_contest(&id)
                .await
                .context("Failed to fetch contest")?;

            println!("\nContest Details: {}", contest.title);
            println!("ID: {}", contest.id);
            println!("Description: {}", contest.description);
            println!("Status: {}", describe_status(&contest));
            println!("\nProblems:");
            println!("{:<10} {:<30} {:<10}", "PROBLEM", "TITLE", "SCORE");
            println!("{}", "-".repeat(52));
            for problem in contest.problems {
                println!(
                    "{:<10} {:<30} {:<10}",
                    problem.id, problem.title, problem.score
                );
            }
        }

        Commands::Problem { id } => {
            let manager = ContestManager::new(&config)?;
            let problem = manager
                .fetch_problem(&id)
                .await
                .context("Failed to fetch problem")?;

            println!("\nProblem {}", problem.title);
            println!("{}", "=".repeat(60));
            println!("Statement:\n{}\n", problem.statement);
            println!("Input Specification:\n{}", problem.input_spec);
            println!("Output Specification:\n{}", problem.output_spec);
            println!("Constraints: {}", problem.constraints);
            println!(
                "Time Limit: {} ms | Memory Limit: {} MB",
                problem.time_limit_ms, problem.memory_limit_mb
            );
        }

        Commands::Run {
            source,
            lang,
            input,
            timeout,
            memory,
        } => {
            let language = resolve_language(lang.as_deref(), &source)?;
            let input_data = read_input(input.as_deref())?;

            println!("Compiling and executing locally ({}) ...", language.name());
            let result = JudgeEngine::run_local_single(
                &config,
                language,
                &source,
                &input_data,
                timeout,
                memory,
            )
            .await
            .context("Local execution error")?;

            println!("\n--- Execution Result ---");
            println!("Status: {:?}", result.status);
            println!("Duration: {} ms", result.duration_ms);
            if let Some(code) = result.exit_code {
                println!("Exit Code: {}", code);
            }
            println!("\n--- STDOUT ---");
            print!("{}", result.stdout);
            if !result.stderr.is_empty() {
                println!("\n--- STDERR ---");
                print!("{}", result.stderr);
            }
        }

        Commands::Submit {
            source,
            problem,
            lang,
            timeout,
            memory,
        } => {
            let language = resolve_language(lang.as_deref(), &source)?;
            let source_code =
                std::fs::read_to_string(&source).context("Could not read source file")?;
            let manager = ContestManager::new(&config)?;

            println!("Downloading problem '{}' from server...", problem);
            let problem_meta = manager
                .fetch_problem(&problem)
                .await
                .context("Failed to fetch problem metadata")?;

            let test_inputs = manager
                .fetch_test_inputs(&problem)
                .await
                .context("Failed to download test inputs")?;

            // Precedence: an explicit flag, else the problem's own limits from
            // the server, else the config defaults. Judging every problem at the
            // client default silently ignores the limits the problem declares.
            let effective_timeout = timeout.or(Some(problem_meta.time_limit_ms));
            let effective_memory = memory.or(Some(problem_meta.memory_limit_mb));

            println!(
                "Executing submission locally across {} test case(s) ({}), limits {} ms / {} MB ...",
                test_inputs.len(),
                language.name(),
                effective_timeout.unwrap_or(config.default_timeout_ms),
                effective_memory.unwrap_or(config.default_memory_limit_mb),
            );
            let submission_result = JudgeEngine::execute_submission(
                &config,
                &problem,
                language,
                &source_code,
                &test_inputs,
                effective_timeout,
                effective_memory,
            )
            .await
            .context("Local execution error")?;

            println!("Submitting local outputs to server for authoritative verification...");
            let api = ApiClient::new(&config);
            let payload = SubmissionPayload {
                problem_id: problem,
                language: language.name().to_string(),
                source_code,
                result: submission_result,
            };

            let verdict = api
                .submit_result(&payload)
                .await
                .context("Submission error")?;

            println!("\n==========================================");
            println!("AUTHORITATIVE VERDICT: {:?}", verdict.verdict);
            println!("==========================================");
            println!("Submission ID: {}", verdict.submission_id);
            println!(
                "Test Cases Passed: {} / {}",
                verdict.passed_test_cases, verdict.total_test_cases
            );
            println!("Details: {}", verdict.message);

            let record = SubmissionRecord {
                submission_id: verdict.submission_id.clone(),
                problem_id: payload.problem_id.clone(),
                language: payload.language.clone(),
                verdict: verdict.verdict.clone(),
                passed_test_cases: verdict.passed_test_cases,
                total_test_cases: verdict.total_test_cases,
                timestamp: Utc::now(),
            };
            // A history write failure shouldn't fail an otherwise successful submission so we log a warning and continue
            if let Err(err) = manager.record_submission(&record) {
                tracing::warn!("Could not record submission to local history: {}", err);
            }
        }

        Commands::History { problem } => {
            let manager = ContestManager::new(&config)?;
            let mut history = manager
                .submission_history()
                .context("Failed to read submission history")?;

            if let Some(problem_id) = &problem {
                history.retain(|record| &record.problem_id == problem_id);
            }
            history.reverse(); // Newest first.

            if history.is_empty() { 
                println!("No submissions recorded yet.");
            } else {
                println!("\nSubmission History:");
                println!(
                    "{:<20} {:<10} {:<6} {:<18} {:<8} {}", // left-align the columns
                    "TIME", "PROBLEM", "LANG", "VERDICT", "PASSED", "SUBMISSION ID"
                );
                // Underline spans the six columns: 20 + 10 + 6 + 18 + 8 + 14 = 82.
                println!("{}", "-".repeat(82));
                for record in history {
                    println!(
                        "{:<20} {:<10} {:<6} {:<18} {:<8} {}",
                        record.timestamp.format("%Y-%m-%d %H:%M:%S"),
                        record.problem_id,
                        record.language,
                        format!("{:?}", record.verdict),
                        format!("{}/{}", record.passed_test_cases, record.total_test_cases),
                        record.submission_id,
                    );
                }
            }
        }

        Commands::MockServer { port } => {
            run_mock_server(port).await?;
        }
    }

    Ok(())
}

fn resolve_language(lang: Option<&str>, source: &Path) -> Result<Language> {
    match lang {
        Some(name) => Language::from_str(name),
        None => Language::from_extension(source),
    }
}

/// user-facing contest status line. Timing is informational, so a malformed server timestamp degrades to a note rather than failing the command..
fn describe_status(contest: &Contest) -> String {
    match contest_timing(contest, Utc::now()) {
        Ok(timing) => match (timing.status, timing.time_remaining) {
            (ContestStatus::Upcoming, Some(remaining)) => {
                format!("UPCOMING (starts in {})", format_duration(remaining))
            }
            (ContestStatus::Running, Some(remaining)) => {
                format!("RUNNING ({} remaining)", format_duration(remaining))
            }
            (ContestStatus::Ended, _) => "ENDED".to_string(),
            (status, None) => format!("{:?}", status),
        },
        Err(err) => format!("unknown ({})", err),
    }
}

fn read_input(input: Option<&Path>) -> Result<String> {
    match input {
        Some(path) => std::fs::read_to_string(path).context("Could not read input file"),
        None => {
            use std::io::Read;
            println!("Enter test input (press Ctrl+D when finished):");
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .context("Could not read stdin")?;
            Ok(buffer)
        }
    }
}
