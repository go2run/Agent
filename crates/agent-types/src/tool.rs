use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Definition of a tool that the LLM can invoke.
/// Follows the OpenAI function-calling schema for broad provider compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub schema_type: String, // always "object"
    pub properties: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub success: bool,
}

/// Shell execution result
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// An entry in a virtual directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// File metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<String>,
}

/// Handle to a running process, used for cancellation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExecHandle(pub u64);
