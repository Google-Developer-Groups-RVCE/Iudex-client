use std::path::{Path, PathBuf};
use std::str::FromStr;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{ClientError, Result};

/// A program and its arguments, ready to hand to `Command::new`.
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
}

/// Static per-language metadata. One literal per variant in [`Language::spec`].
struct Spec {
    name: &'static str,
    source_filename: &'static str,
    aliases: &'static [&'static str],
    extensions: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Cpp,
    Java,
    Python,
}

impl Language {
    const ALL: [Language; 3] = [Language::Cpp, Language::Java, Language::Python];

    fn spec(self) -> Spec {
        match self {
            Language::Cpp => Spec {
                name: "cpp",
                source_filename: "solution.cpp",
                aliases: &["cpp", "c++", "cxx", "cc"],
                extensions: &["cpp", "cxx", "cc", "c++"],
            },
            Language::Java => Spec {
                name: "java",
                source_filename: "Solution.java",
                aliases: &["java"],
                extensions: &["java"],
            },
            Language::Python => Spec {
                name: "python",
                source_filename: "solution.py",
                aliases: &["python", "py", "python3"],
                extensions: &["py"],
            },
        }
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub fn source_filename(self) -> &'static str {
        self.spec().source_filename
    }

    /// Where the source is written inside a judging workspace. Every command is
    /// built from `work_dir`, so this is derived rather than passed around.
    pub fn source_path(self, work_dir: &Path) -> PathBuf {
        work_dir.join(self.source_filename())
    }

    pub fn from_extension(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Self::ALL
            .into_iter()
            .find(|l| l.spec().extensions.contains(&ext.as_str()))
            .ok_or_else(|| {
                ClientError::UnsupportedLanguage(format!("Unknown file extension '.{}'", ext))
            })
    }

    /// `None` for interpreted languages, which have no compile step.
    pub fn compile(self, config: &Config, work_dir: &Path) -> Option<Invocation> {
        let source = self.source_path(work_dir);
        match self {
            Language::Cpp => Some(Invocation {
                program: config.cpp_compiler.clone(),
                args: vec![
                    "-O2".to_string(),
                    "-std=c++17".to_string(),
                    source.to_string_lossy().to_string(),
                    "-o".to_string(),
                    work_dir.join("solution").to_string_lossy().to_string(),
                ],
            }),
            Language::Java => Some(Invocation {
                program: config.java_compiler.clone(),
                args: vec![
                    source.to_string_lossy().to_string(),
                    "-d".to_string(),
                    work_dir.to_string_lossy().to_string(),
                ],
            }),
            Language::Python => None,
        }
    }

    pub fn execute(self, config: &Config, work_dir: &Path) -> Invocation {
        match self {
            Language::Cpp => Invocation {
                program: work_dir.join("solution").to_string_lossy().to_string(),
                args: vec![],
            },
            Language::Java => Invocation {
                program: config.java_runner.clone(),
                args: vec![
                    "-cp".to_string(),
                    work_dir.to_string_lossy().to_string(),
                    "Solution".to_string(),
                ],
            },
            Language::Python => Invocation {
                program: config.python_interpreter.clone(),
                args: vec![self.source_path(work_dir).to_string_lossy().to_string()],
            },
        }
    }
}

impl FromStr for Language {
    type Err = ClientError;

    fn from_str(lang: &str) -> Result<Self> {
        let lang = lang.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|l| l.spec().aliases.contains(&lang.as_str()))
            .ok_or(ClientError::UnsupportedLanguage(lang))
    }
}
