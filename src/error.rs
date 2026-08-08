use thiserror::Error;

#[derive(Error, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ClientError {
    #[error("Compilation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}")]
    CompilationFailed { stdout: String, stderr: String },

    #[error("Execution timed out after {limit_ms}ms")]
    ExecutionTimeout { limit_ms: u64 },

    #[error("Output limit exceeded ({limit_bytes} bytes max)")]
    OutputLimitExceeded { limit_bytes: usize },

    #[error("Compiler/interpreter for '{language}' not found (tried binary '{binary}')")]
    CompilerNotFound { language: String, binary: String },

    #[error("Unsupported programming language: {0}")]
    UnsupportedLanguage(String),

    #[error("API Error (status {status_code:?}): {message}")]
    ApiError {
        message: String,
        status_code: Option<u16>,
    },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Network error: {0}")]
    Network(String),
}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        ClientError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> Self {
        ClientError::Serialization(err.to_string())
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(err: reqwest::Error) -> Self {
        ClientError::Network(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;
