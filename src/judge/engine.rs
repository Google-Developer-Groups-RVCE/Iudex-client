use std::path::Path;
use tempfile::TempDir;
use tracing::info;

use crate::api::models::TestInput;
use crate::config::Config;
use crate::error::{ClientError, Result};
use crate::judge::compiler::Compiler;
use crate::judge::result::{ExecutionTelemetry, SubmissionResult, TestResult};
use crate::judge::runner::ProcessRunner;
use crate::languages::Language;

pub struct JudgeEngine;

impl JudgeEngine {
    pub async fn execute_submission(
        config: &Config,
        problem_id: &str,
        language: Language,
        source_code: &str,
        test_inputs: &[TestInput],
        timeout_ms: Option<u64>,
    ) -> Result<SubmissionResult> {
        let temp_dir = TempDir::new()?;
        let work_path = temp_dir.path();

        let source_filename = language.source_filename();
        let source_path = work_path.join(source_filename);
        std::fs::write(&source_path, source_code)?;

        info!("Created workspace at {:?}", work_path);

        let compilation_res = Compiler::compile(config, language, &source_path, work_path).await?;

        if !compilation_res.success {
            let telemetry = Self::collect_telemetry(config, language);
            return Ok(SubmissionResult {
                problem_id: problem_id.to_string(),
                language: language.name().to_string(),
                compilation: Some(compilation_res),
                test_results: vec![],
                telemetry,
            });
        }

        let effective_timeout = timeout_ms.unwrap_or(config.default_timeout_ms);
        let mut test_results = Vec::new();

        for test in test_inputs {
            info!("Running test case '{}'", test.id);
            let result = ProcessRunner::run_test_case(
                config,
                language,
                &source_path,
                work_path,
                &test.id,
                &test.input_data,
                effective_timeout,
            )
            .await?;
            test_results.push(result);
        }

        let telemetry = Self::collect_telemetry(config, language);

        // temp_dir automatically cleans up here!
        Ok(SubmissionResult {
            problem_id: problem_id.to_string(),
            language: language.name().to_string(),
            compilation: if language.needs_compilation() {
                Some(compilation_res)
            } else {
                None
            },
            test_results,
            telemetry,
        })
    }

    pub async fn run_local_single(
        config: &Config,
        language: Language,
        source_path: &Path,
        input_data: &str,
        timeout_ms: Option<u64>,
    ) -> Result<TestResult> {
        let temp_dir = TempDir::new()?;
        let work_path = temp_dir.path();

        let target_source_path = work_path.join(language.source_filename());
        std::fs::copy(source_path, &target_source_path)?;

        let compilation_res = Compiler::compile(config, language, &target_source_path, work_path).await?;

        if !compilation_res.success {
            return Err(ClientError::CompilationFailed {
                stdout: compilation_res.stdout,
                stderr: compilation_res.stderr,
            });
        }

        let effective_timeout = timeout_ms.unwrap_or(config.default_timeout_ms);
        ProcessRunner::run_test_case(
            config,
            language,
            &target_source_path,
            work_path,
            "local_run",
            input_data,
            effective_timeout,
        )
        .await
    }

    fn collect_telemetry(config: &Config, language: Language) -> ExecutionTelemetry {
        let compiler_name = match language {
            Language::Cpp => config.cpp_compiler.clone(),
            Language::Java => config.java_compiler.clone(),
            Language::Python => config.python_interpreter.clone(),
        };

        ExecutionTelemetry {
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            compiler: compiler_name,
        }
    }
}
