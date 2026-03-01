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

/// Commands sent from main thread to the Wasmer-JS worker
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerCommand {
    /// Initialize the Wasmer-JS runtime
    Init,
    /// Execute a bash command
    ExecBash {
        id: u64,
        cmd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Execute a command from a specific WASIX package (auto-installs from registry)
    ExecPackage {
        id: u64,
        /// Wasmer registry package name, e.g. "sharrattj/coreutils"
        package: String,
        /// Arguments to pass to the package entrypoint
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Pre-install a WASIX package from the registry (response: PackageInstalled)
    InstallPackage {
        id: u64,
        /// Wasmer registry package name
        package: String,
    },
    /// Cancel a running execution
    CancelExec { id: u64 },
    /// Write to stdin of a running process
    WriteStdin { id: u64, data: String },
    /// List cached packages
    ListPackages { id: u64 },
}

/// Events from the worker back to main thread
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerEvent {
    /// Worker initialized successfully
    Ready,
    /// stdout data from a process
    Stdout { id: u64, data: String },
    /// stderr data from a process
    Stderr { id: u64, data: String },
    /// Process exited
    ExitCode { id: u64, code: i32 },
    /// An error occurred in the worker
    Error { id: u64, message: String },
    /// A package was installed (or was already cached)
    PackageInstalled { id: u64, package: String, cached: bool },
    /// List of cached package names
    PackageList { id: u64, packages: Vec<String> },
}
