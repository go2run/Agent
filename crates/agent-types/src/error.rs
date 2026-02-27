use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Shell error: {0}")]
    Shell(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Filesystem error: {path}: {message}")]
    Fs { path: String, message: String },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Cancelled")]
    Cancelled,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("JS interop error: {0}")]
    JsInterop(String),

    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for AgentError {
    fn from(e: serde_json::Error) -> Self {
        AgentError::Serialization(e.to_string())
    }
}
