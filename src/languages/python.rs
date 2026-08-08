use std::path::Path;
use crate::config::Config;

pub struct PythonLanguage;

impl PythonLanguage {
    pub fn name(&self) -> &'static str {
        "python"
    }

    pub fn source_filename(&self) -> &'static str {
        "solution.py"
    }

    pub fn needs_compilation(&self) -> bool {
        false
    }

    pub fn execute_command(&self, config: &Config, source_path: &Path) -> (String, Vec<String>) {
        let interpreter = config.python_interpreter.clone();
        let args = vec![source_path.to_string_lossy().to_string()];
        (interpreter, args)
    }
}
