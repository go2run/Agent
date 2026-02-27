//! Port traits — the hexagonal architecture boundary.
//!
//! These traits are defined here in `agent-core` (pure Rust).
//! Implementations live in `agent-platform` (browser adapters).
//! The core never imports platform code; it only depends on these traits.

use std::pin::Pin;
use async_trait::async_trait;
use futures::Stream;
use agent_types::{
    Result,
    message::Message,
    tool::{DirEntry, ExecHandle, ExecResult, FileStat, ToolDefinition},
};

// ─── LLM Port ────────────────────────────────────────────────

/// Streaming event from an LLM response
#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    /// A partial token
    Delta(String),
    /// A tool call is being assembled (partial JSON)
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    /// Stream finished
    Done,
    /// Error during streaming
    Error(String),
}

/// Request to send to an LLM
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// Complete (non-streaming) response from an LLM
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[async_trait(?Send)]
pub trait LlmPort {
    /// Non-streaming chat completion
    async fn chat_completion(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// Streaming chat completion — returns a stream of events
    fn stream_chat(
        &self,
        req: ChatRequest,
    ) -> Pin<Box<dyn Stream<Item = LlmStreamEvent>>>;

    /// List available models for this provider
    async fn list_models(&self) -> Result<Vec<String>>;
}

// ─── Shell Port ──────────────────────────────────────────────

#[async_trait(?Send)]
pub trait ShellPort {
    /// Execute a command and return the full result
    async fn execute(&self, cmd: &str, timeout_ms: Option<u64>) -> Result<ExecResult>;

    /// Execute a command with streaming output
    fn execute_streaming(
        &self,
        cmd: &str,
    ) -> Pin<Box<dyn Stream<Item = ShellStreamEvent>>>;

    /// Cancel a running execution
    async fn cancel(&self, handle: ExecHandle) -> Result<()>;

    /// Check if the shell runtime is ready
    fn is_ready(&self) -> bool;
}

#[derive(Debug, Clone)]
pub enum ShellStreamEvent {
    Stdout(String),
    Stderr(String),
    Exit(i32),
    Error(String),
}

// ─── Storage Port ────────────────────────────────────────────

#[async_trait(?Send)]
pub trait StoragePort {
    /// Get a value by key
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Set a value
    async fn set(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Delete a value
    async fn delete(&self, key: &str) -> Result<()>;

    /// List keys with a given prefix
    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>>;

    /// Check if a key exists
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }

    /// Name of this backend (for logging/debug)
    fn backend_name(&self) -> &str;
}

// ─── Virtual Filesystem Port ─────────────────────────────────

#[async_trait(?Send)]
pub trait VfsPort {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn delete_file(&self, path: &str) -> Result<()>;
    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>>;
    async fn stat(&self, path: &str) -> Result<FileStat>;
    async fn mkdir(&self, path: &str) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
}
