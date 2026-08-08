use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

use crate::error::{ClientError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_url: String,
    pub auth_token: Option<String>,
    pub username: Option<String>,
    pub cpp_compiler: String,
    pub java_compiler: String,
    pub java_runner: String,
    pub python_interpreter: String,
    pub default_timeout_ms: u64,
    pub max_output_bytes: usize,
    pub temp_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".to_string(),
            auth_token: None,
            username: None,
            cpp_compiler: detect_binary(&["g++", "clang++"]).unwrap_or_else(|| "g++".to_string()),
            java_compiler: detect_binary(&["javac"]).unwrap_or_else(|| "javac".to_string()),
            java_runner: detect_binary(&["java"]).unwrap_or_else(|| "java".to_string()),
            python_interpreter: detect_binary(&["python3", "python"]).unwrap_or_else(|| "python3".to_string()),
            default_timeout_ms: 5000,
            max_output_bytes: 10 * 1024 * 1024, // 10MB limit
            temp_dir: None,
        }
    }
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| ClientError::ConfigError("Could not determine user home directory".to_string()))?;
        let path = PathBuf::from(home).join(".config").join("cp-client").join("config.json");
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            let default_config = Config::default();
            let _ = default_config.save();
            Ok(default_config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        info!("Configuration saved.");
        Ok(())
    }
}

fn detect_binary(candidates: &[&str]) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for candidate in candidates {
        for dir in std::env::split_paths(&path_var) {
            let bin_path = dir.join(candidate);
            #[cfg(unix)]
            if bin_path.is_file() {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = bin_path.metadata() {
                    if metadata.permissions().mode() & 0o111 != 0 {
                        return Some(candidate.to_string());
                    }
                }
            }
            #[cfg(windows)]
            {
                let exe_path = dir.join(format!("{}.exe", candidate));
                if bin_path.is_file() || exe_path.is_file() {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}
