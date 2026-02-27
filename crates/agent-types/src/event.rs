use serde::{Deserialize, Serialize};

/// Events emitted by the agent runtime.
/// UI subscribes to these for reactive updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// Agent started processing a user message
    TurnStart { turn_id: u64 },

    /// LLM is producing tokens
    LlmDelta { token: String },

    /// LLM finished a complete response
    LlmComplete { text: String },

    /// A tool call is about to execute
    ToolExecStart { call_id: String, tool_name: String, arguments: String },

    /// Streaming output from a tool (e.g., bash stdout)
    ToolOutput { call_id: String, chunk: String },

    /// Tool execution finished
    ToolExecEnd { call_id: String, result: String, success: bool },

    /// Agent finished the current turn
    TurnEnd { turn_id: u64 },

    /// An error occurred
    Error { message: String },
}

/// Events from the Wasmer-JS worker thread
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerCommand {
    /// Execute a bash command
    ExecBash {
        id: u64,
        cmd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Cancel a running execution
    CancelExec { id: u64 },
    /// Write to stdin of a running process
    WriteStdin { id: u64, data: String },
    /// Initialize the Wasmer-JS runtime
    Init,
}

/// Events from the worker back to main thread
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerEvent {
    /// Worker initialized successfully
    Ready,
    /// stdout data from a bash process
    Stdout { id: u64, data: String },
    /// stderr data from a bash process
    Stderr { id: u64, data: String },
    /// Process exited
    ExitCode { id: u64, code: i32 },
    /// An error occurred in the worker
    Error { id: u64, message: String },
}
