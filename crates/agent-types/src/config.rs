use serde::{Deserialize, Serialize};

/// Top-level agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm: LlmConfig,
    pub storage: StorageConfig,
    pub system_prompt: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            storage: StorageConfig::default(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub api_key: String,
    pub api_base: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::DeepSeek,
            model: "deepseek-chat".to_string(),
            api_key: String::new(),
            api_base: None,
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmProvider {
    DeepSeek,
    OpenAI,
    Anthropic,
    Google,
    Custom,
}

impl LlmProvider {
    pub fn default_base_url(&self) -> &str {
        match self {
            LlmProvider::DeepSeek => "https://api.deepseek.com",
            LlmProvider::OpenAI => "https://api.openai.com",
            LlmProvider::Anthropic => "https://api.anthropic.com",
            LlmProvider::Google => "https://generativelanguage.googleapis.com",
            LlmProvider::Custom => "",
        }
    }

    pub fn all() -> &'static [LlmProvider] {
        &[
            LlmProvider::DeepSeek,
            LlmProvider::OpenAI,
            LlmProvider::Anthropic,
            LlmProvider::Google,
            LlmProvider::Custom,
        ]
    }

    pub fn label(&self) -> &str {
        match self {
            LlmProvider::DeepSeek => "DeepSeek",
            LlmProvider::OpenAI => "OpenAI",
            LlmProvider::Anthropic => "Anthropic",
            LlmProvider::Google => "Google",
            LlmProvider::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend: StorageBackendType,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendType::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageBackendType {
    /// Auto-detect best available backend
    Auto,
    Memory,
    IndexedDb,
    Opfs,
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are an AI agent running inside a browser-based WASM environment.
You have access to a virtual filesystem and a bash shell (via WASIX/Wasmer).

Available tools:
- bash: Execute shell commands
- read_file: Read file contents
- write_file: Write content to a file
- list_dir: List directory contents

When the user asks you to perform tasks, use the appropriate tools.
Always explain what you're doing before executing commands.
"#;
