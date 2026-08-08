use std::path::Path;
use crate::config::Config;

pub struct JavaLanguage;

impl JavaLanguage {
    pub fn name(&self) -> &'static str {
        "java"
    }

    pub fn source_filename(&self) -> &'static str {
        "Solution.java"
    }

    pub fn needs_compilation(&self) -> bool {
        true
    }

    pub fn compile_command(&self, config: &Config, source_path: &Path, output_dir: &Path) -> (String, Vec<String>) {
        let compiler = config.java_compiler.clone();
        let args = vec![
            source_path.to_string_lossy().to_string(),
            "-d".to_string(),
            output_dir.to_string_lossy().to_string(),
        ];
        (compiler, args)
    }

    pub fn execute_command(&self, config: &Config, work_dir: &Path) -> (String, Vec<String>) {
        let runner = config.java_runner.clone();
        let args = vec![
            "-cp".to_string(),
            work_dir.to_string_lossy().to_string(),
            "Solution".to_string(),
        ];
        (runner, args)
    }
}
