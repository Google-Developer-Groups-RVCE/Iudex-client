pub mod cpp;
pub mod java;
pub mod python;

use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{ClientError, Result};
use cpp::CppLanguage;
use java::JavaLanguage;
use python::PythonLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Cpp,
    Java,
    Python,
}

impl Language {
    pub fn from_str(lang: &str) -> Result<Self> {
        match lang.to_lowercase().as_str() {
            "cpp" | "c++" | "cxx" | "cc" => Ok(Language::Cpp),
            "java" => Ok(Language::Java),
            "python" | "py" | "python3" => Ok(Language::Python),
            other => Err(ClientError::UnsupportedLanguage(other.to_string())),
        }
    }

    pub fn from_extension(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match ext.to_lowercase().as_str() {
            "cpp" | "cxx" | "cc" | "c++" => Ok(Language::Cpp),
            "java" => Ok(Language::Java),
            "py" => Ok(Language::Python),
            other => Err(ClientError::UnsupportedLanguage(format!(
                "Unknown file extension '.{}'",
                other
            ))),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Language::Cpp => CppLanguage.name(),
            Language::Java => JavaLanguage.name(),
            Language::Python => PythonLanguage.name(),
        }
    }

    pub fn source_filename(&self) -> &'static str {
        match self {
            Language::Cpp => CppLanguage.source_filename(),
            Language::Java => JavaLanguage.source_filename(),
            Language::Python => PythonLanguage.source_filename(),
        }
    }

    pub fn needs_compilation(&self) -> bool {
        match self {
            Language::Cpp => CppLanguage.needs_compilation(),
            Language::Java => JavaLanguage.needs_compilation(),
            Language::Python => PythonLanguage.needs_compilation(),
        }
    }

    pub fn compile_command(&self, config: &Config, source_path: &Path, output_dir: &Path) -> Option<(String, Vec<String>)> {
        match self {
            Language::Cpp => Some(CppLanguage.compile_command(config, source_path, output_dir)),
            Language::Java => Some(JavaLanguage.compile_command(config, source_path, output_dir)),
            Language::Python => None,
        }
    }

    pub fn execute_command(&self, config: &Config, source_path: &Path, work_dir: &Path) -> (String, Vec<String>) {
        match self {
            Language::Cpp => CppLanguage.execute_command(config, work_dir),
            Language::Java => JavaLanguage.execute_command(config, work_dir),
            Language::Python => PythonLanguage.execute_command(config, source_path),
        }
    }
}
