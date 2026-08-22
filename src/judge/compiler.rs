use std::path::Path;
use std::time::Instant;
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::Config;
use crate::error::{ClientError, Result};
use crate::judge::result::CompilationResult;
use crate::languages::Language;

pub struct Compiler;

impl Compiler {
    /// Returns `Ok(None)` when the language has no compile step at all, which is
    /// the single source of truth for "was this compiled?".
    pub async fn compile(
        config: &Config,
        language: Language,
        work_dir: &Path,
    ) -> Result<Option<CompilationResult>> {
        let Some(invocation) = language.compile(config, work_dir) else {
            return Ok(None);
        };

        info!(
            "Compiling with command: {} {}",
            invocation.program,
            invocation.args.join(" ")
        );
        let start = Instant::now();

        let child = Command::new(&invocation.program)
            .args(&invocation.args)
            .current_dir(work_dir)
            .output()
            .await
            .map_err(|_e| ClientError::CompilerNotFound {
                language: language.name().to_string(),
                binary: invocation.program.clone(),
            })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&child.stdout).to_string();
        let stderr = String::from_utf8_lossy(&child.stderr).to_string();
        let success = child.status.success();

        if !success {
            warn!("Compilation failed for language {}", language.name());
        } else {
            info!("Compilation succeeded in {}ms", duration_ms);
        }

        Ok(Some(CompilationResult {
            success,
            stdout,
            stderr,
            duration_ms,
        }))
    }
}
