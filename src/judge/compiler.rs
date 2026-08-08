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
    pub async fn compile(
        config: &Config,
        language: Language,
        source_path: &Path,
        work_dir: &Path,
    ) -> Result<CompilationResult> {
        let (cmd_bin, args) = match language.compile_command(config, source_path, work_dir) {
            Some(cmd) => cmd,
            None => {
                return Ok(CompilationResult {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: 0,
                });
            }
        };

        info!("Compiling with command: {} {}", cmd_bin, args.join(" "));
        let start = Instant::now();

        let child = Command::new(&cmd_bin)
            .args(&args)
            .current_dir(work_dir)
            .output()
            .await
            .map_err(|_e| ClientError::CompilerNotFound {
                language: language.name().to_string(),
                binary: cmd_bin.clone(),
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

        Ok(CompilationResult {
            success,
            stdout,
            stderr,
            duration_ms,
        })
    }
}
