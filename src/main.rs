use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;
use tracing_subscriber::EnvFilter;

use cp_client::api::client::ApiClient;
use cp_client::api::models::SubmissionPayload;
use cp_client::config::Config;
use cp_client::contest::manager::ContestManager;
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
    },

    /// Start a local mock contest server for development and testing
    MockServer {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::WARN.into()))
        .init();

    let cli = Cli::parse();
    let mut config = Config::load().unwrap_or_default();

    match cli.command {
        Commands::Login {
            server,
            username,
            password,
        } => {
            println!("🔑 Logging into server {} as '{}'...", server, username);
            config.server_url = server;
            let api = ApiClient::new(&config);

            match api.login(&username, &password).await {
                Ok(resp) => {
                    config.auth_token = Some(resp.token.clone());
                    config.username = Some(resp.username.clone());
                    config.save()?;
                    println!("✅ Login successful! Session saved for user '{}'.", resp.username);
                }
                Err(err) => {
                    eprintln!("❌ Login failed: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Contests => {
            let manager = ContestManager::new(&config)?;
            match manager.list_contests().await {
                Ok(contests) => {
                    println!("\n🏆 Available Contests:");
                    println!("{:<15} {:<30} {:<20}", "CONTEST ID", "TITLE", "PROBLEMS");
                    println!("{}", "-".repeat(68));
                    for c in contests {
                        println!("{:<15} {:<30} {:<20}", c.id, c.title, c.problems.len());
                    }
                }
                Err(err) => {
                    eprintln!("❌ Failed to list contests: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Contest { id } => {
            let manager = ContestManager::new(&config)?;
            match manager.get_contest(&id).await {
                Ok(contest) => {
                    println!("\n📌 Contest Details: {}", contest.title);
                    println!("ID: {}", contest.id);
                    println!("Description: {}", contest.description);
                    println!("\nProblems:");
                    println!("{:<10} {:<30} {:<10}", "PROBLEM", "TITLE", "SCORE");
                    println!("{}", "-".repeat(52));
                    for p in contest.problems {
                        println!("{:<10} {:<30} {:<10}", p.id, p.title, p.score);
                    }
                }
                Err(err) => {
                    eprintln!("❌ Failed to fetch contest: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Problem { id } => {
            let manager = ContestManager::new(&config)?;
            match manager.get_problem(&id).await {
                Ok(prob) => {
                    println!("\n📄 Problem {}", prob.title);
                    println!("{}", "=".repeat(60));
                    println!("Statement:\n{}\n", prob.statement);
                    println!("Input Specification:\n{}", prob.input_spec);
                    println!("Output Specification:\n{}", prob.output_spec);
                    println!("Constraints: {}", prob.constraints);
                    println!("Time Limit: {} ms | Memory Limit: {} MB", prob.time_limit_ms, prob.memory_limit_mb);
                }
                Err(err) => {
                    eprintln!("❌ Failed to fetch problem: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Run {
            source,
            lang,
            input,
            timeout,
        } => {
            let language = match lang {
                Some(l) => Language::from_str(&l)?,
                None => Language::from_extension(&source)?,
            };

            let input_data = match input {
                Some(path) => std::fs::read_to_string(path)?,
                None => {
                    use std::io::Read;
                    println!("Enter test input (press Ctrl+D when finished):");
                    let mut buffer = String::new();
                    std::io::stdin().read_to_string(&mut buffer)?;
                    buffer
                }
            };

            println!("⚡ Compiling and executing locally ({}) ...", language.name());
            match JudgeEngine::run_local_single(&config, language, &source, &input_data, timeout).await {
                Ok(result) => {
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
                Err(err) => {
                    eprintln!("❌ Local execution error: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Submit {
            source,
            problem,
            lang,
            timeout,
        } => {
            let language = match lang {
                Some(l) => Language::from_str(&l)?,
                None => Language::from_extension(&source)?,
            };

            let source_code = std::fs::read_to_string(&source)?;
            let manager = ContestManager::new(&config)?;

            println!("📥 Downloading test inputs for problem '{}' from server...", problem);
            let test_inputs = match manager.fetch_test_inputs(&problem).await {
                Ok(inputs) => inputs,
                Err(err) => {
                    eprintln!("❌ Failed to download test inputs: {}", err);
                    std::process::exit(1);
                }
            };

            println!(
                "⚡ Executing submission locally across {} test case(s) ({}) ...",
                test_inputs.len(),
                language.name()
            );

            let submission_result = JudgeEngine::execute_submission(
                &config,
                &problem,
                language,
                &source_code,
                &test_inputs,
                timeout,
            )
            .await?;

            println!("📤 Submitting local outputs to server for authoritative verification...");
            let api = ApiClient::new(&config);
            let payload = SubmissionPayload {
                problem_id: problem,
                language: language.name().to_string(),
                source_code,
                result: submission_result,
            };

            match api.submit_result(&payload).await {
                Ok(verdict) => {
                    println!("\n==========================================");
                    println!("🏆 AUTHORITATIVE VERDICT: {:?}", verdict.verdict);
                    println!("==========================================");
                    println!("Submission ID: {}", verdict.submission_id);
                    println!("Test Cases Passed: {} / {}", verdict.passed_test_cases, verdict.total_test_cases);
                    println!("Details: {}", verdict.message);
                }
                Err(err) => {
                    eprintln!("❌ Submission error: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::MockServer { port } => {
            run_mock_server(port).await?;
        }
    }

    Ok(())
}
