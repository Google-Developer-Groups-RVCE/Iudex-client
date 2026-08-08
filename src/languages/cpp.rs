use std::path::Path;
use crate::config::Config;

pub struct CppLanguage;

impl CppLanguage {
    pub fn name(&self) -> &'static str {
        "cpp"
    }

    pub fn source_filename(&self) -> &'static str {
        "solution.cpp"
    }

    pub fn needs_compilation(&self) -> bool {
        true
    }

    pub fn compile_command(&self, config: &Config, source_path: &Path, output_dir: &Path) -> (String, Vec<String>) {
        let exe_path = output_dir.join("solution");
        let compiler = config.cpp_compiler.clone();
        let args = vec![
            "-O2".to_string(),
            "-std=c++17".to_string(),
            source_path.to_string_lossy().to_string(),
            "-o".to_string(),
            exe_path.to_string_lossy().to_string(),
        ];
        (compiler, args)
    }

    pub fn execute_command(&self, _config: &Config, work_dir: &Path) -> (String, Vec<String>) {
        let exe_path = work_dir.join("solution");
        (exe_path.to_string_lossy().to_string(), vec![])
    }
}
