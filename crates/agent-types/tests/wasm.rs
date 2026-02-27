//! WASM-target tests for agent-types.
//!
//! Mirrors the native unit tests but runs under wasm32-unknown-unknown
//! via `wasm-pack test --node`.

use wasm_bindgen_test::*;

use agent_types::message::*;
use agent_types::event::*;
use agent_types::tool::*;
use agent_types::config::*;
use agent_types::session::*;
use agent_types::error::*;

// ─── Message Tests ───────────────────────────────────────

#[wasm_bindgen_test]
fn message_system() {
    let msg = Message::system("You are an agent");
    assert_eq!(msg.role, Role::System);
    assert_eq!(msg.content.as_text(), "You are an agent");
    assert!(msg.tool_call_id.is_none());
    assert!(msg.tool_calls.is_empty());
}

#[wasm_bindgen_test]
fn message_user() {
    let msg = Message::user("Hello");
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content.as_text(), "Hello");
}

#[wasm_bindgen_test]
fn message_assistant() {
    let msg = Message::assistant("I can help");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content.as_text(), "I can help");
}

#[wasm_bindgen_test]
fn message_tool_result() {
    let msg = Message::tool_result("call_123", "output data");
    assert_eq!(msg.role, Role::Tool);
    assert_eq!(msg.content.as_text(), "output data");
    assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
}

#[wasm_bindgen_test]
fn message_serialization_roundtrip() {
    let msg = Message::user("test input");
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, Role::User);
    assert_eq!(deserialized.content.as_text(), "test input");
}

#[wasm_bindgen_test]
fn message_with_tool_calls_serialization() {
    let msg = Message {
        role: Role::Assistant,
        content: MessageContent::Text(String::new()),
        tool_call_id: None,
        tool_calls: vec![ToolCallRequest {
            id: "call_1".to_string(),
            function: FunctionCall {
                name: "bash".to_string(),
                arguments: r#"{"command":"ls"}"#.to_string(),
            },
        }],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("bash"));
    assert!(json.contains("call_1"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tool_calls.len(), 1);
    assert_eq!(deserialized.tool_calls[0].function.name, "bash");
}

#[wasm_bindgen_test]
fn message_content_text() {
    let content = MessageContent::Text("hello".to_string());
    assert_eq!(content.as_text(), "hello");
}

#[wasm_bindgen_test]
fn message_content_parts() {
    let content = MessageContent::Parts(vec![
        ContentPart::Text { text: "part1".to_string() },
    ]);
    assert_eq!(content.as_text(), "part1");
}

#[wasm_bindgen_test]
fn message_content_empty_parts() {
    let content = MessageContent::Parts(vec![]);
    assert_eq!(content.as_text(), "");
}

// ─── Role Serialization ──────────────────────────────────

#[wasm_bindgen_test]
fn role_serialization() {
    assert_eq!(serde_json::to_string(&Role::System).unwrap(), r#""system""#);
    assert_eq!(serde_json::to_string(&Role::User).unwrap(), r#""user""#);
    assert_eq!(serde_json::to_string(&Role::Assistant).unwrap(), r#""assistant""#);
    assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), r#""tool""#);
}

#[wasm_bindgen_test]
fn role_deserialization() {
    let role: Role = serde_json::from_str(r#""system""#).unwrap();
    assert_eq!(role, Role::System);
}

// ─── Event Tests ─────────────────────────────────────────

#[wasm_bindgen_test]
fn agent_event_serialization() {
    let event = AgentEvent::TurnStart { turn_id: 1 };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("TurnStart"));
}

#[wasm_bindgen_test]
fn agent_event_llm_complete() {
    let event = AgentEvent::LlmComplete { text: "Hello world".to_string() };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("Hello world"));
}

#[wasm_bindgen_test]
fn agent_event_tool_exec() {
    let event = AgentEvent::ToolExecStart {
        call_id: "c1".to_string(),
        tool_name: "bash".to_string(),
        arguments: r#"{"command":"ls"}"#.to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("bash"));
    assert!(json.contains("c1"));
}

#[wasm_bindgen_test]
fn worker_command_serialization() {
    let cmd = WorkerCommand::ExecBash {
        id: 42,
        cmd: "echo hello".to_string(),
        timeout_ms: Some(5000),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("ExecBash"));
    assert!(json.contains("echo hello"));
    assert!(json.contains("5000"));
}

#[wasm_bindgen_test]
fn worker_event_roundtrip() {
    let event = WorkerEvent::Stdout {
        id: 1,
        data: "output".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: WorkerEvent = serde_json::from_str(&json).unwrap();
    if let WorkerEvent::Stdout { id, data } = deserialized {
        assert_eq!(id, 1);
        assert_eq!(data, "output");
    } else {
        panic!("Wrong variant");
    }
}

// ─── Tool Tests ──────────────────────────────────────────

#[wasm_bindgen_test]
fn tool_definition_serialization() {
    let tool = ToolDefinition {
        name: "bash".to_string(),
        description: "Execute bash".to_string(),
        parameters: ToolParameters {
            schema_type: "object".to_string(),
            properties: serde_json::Map::new(),
            required: vec!["command".to_string()],
        },
    };
    let json = serde_json::to_string(&tool).unwrap();
    let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "bash");
    assert_eq!(deserialized.parameters.required, vec!["command"]);
}

#[wasm_bindgen_test]
fn exec_result_fields() {
    let result = ExecResult {
        stdout: "hello\n".to_string(),
        stderr: String::new(),
        exit_code: 0,
    };
    assert_eq!(result.exit_code, 0);
    assert!(result.stderr.is_empty());
}

#[wasm_bindgen_test]
fn dir_entry_serialization() {
    let entry = DirEntry {
        name: "file.txt".to_string(),
        is_dir: false,
        size: 1024,
    };
    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: DirEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "file.txt");
    assert!(!deserialized.is_dir);
    assert_eq!(deserialized.size, 1024);
}

#[wasm_bindgen_test]
fn file_stat_serialization() {
    let stat = FileStat {
        size: 2048,
        is_dir: true,
        modified: Some("2026-01-01T00:00:00Z".to_string()),
    };
    let json = serde_json::to_string(&stat).unwrap();
    let deserialized: FileStat = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.size, 2048);
    assert!(deserialized.is_dir);
    assert!(deserialized.modified.is_some());
}

#[wasm_bindgen_test]
fn exec_handle_equality() {
    let h1 = ExecHandle(1);
    let h2 = ExecHandle(1);
    let h3 = ExecHandle(2);
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

// ─── Config Tests ────────────────────────────────────────

#[wasm_bindgen_test]
fn default_config() {
    let config = AgentConfig::default();
    assert_eq!(config.llm.provider, LlmProvider::DeepSeek);
    assert_eq!(config.llm.model, "deepseek-chat");
    assert!(config.llm.api_key.is_empty());
    assert!(config.llm.api_base.is_none());
    assert_eq!(config.llm.max_tokens, 4096);
    assert!(!config.system_prompt.is_empty());
}

#[wasm_bindgen_test]
fn config_serialization_roundtrip() {
    let config = AgentConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.llm.provider, LlmProvider::DeepSeek);
    assert_eq!(deserialized.llm.model, "deepseek-chat");
}

#[wasm_bindgen_test]
fn llm_provider_base_urls() {
    assert_eq!(LlmProvider::DeepSeek.default_base_url(), "https://api.deepseek.com");
    assert_eq!(LlmProvider::OpenAI.default_base_url(), "https://api.openai.com");
    assert_eq!(LlmProvider::Anthropic.default_base_url(), "https://api.anthropic.com");
    assert!(!LlmProvider::Google.default_base_url().is_empty());
}

#[wasm_bindgen_test]
fn llm_provider_labels() {
    assert_eq!(LlmProvider::DeepSeek.label(), "DeepSeek");
    assert_eq!(LlmProvider::OpenAI.label(), "OpenAI");
    assert_eq!(LlmProvider::Anthropic.label(), "Anthropic");
    assert_eq!(LlmProvider::Google.label(), "Google");
    assert_eq!(LlmProvider::Custom.label(), "Custom");
}

#[wasm_bindgen_test]
fn llm_provider_all() {
    let all = LlmProvider::all();
    assert_eq!(all.len(), 5);
    assert!(all.contains(&LlmProvider::DeepSeek));
    assert!(all.contains(&LlmProvider::OpenAI));
}

#[wasm_bindgen_test]
fn storage_backend_type_default() {
    let config = StorageConfig::default();
    assert_eq!(config.backend, StorageBackendType::Auto);
}

// ─── Session Tests ───────────────────────────────────────

#[wasm_bindgen_test]
fn session_new() {
    let session = Session::new("test-id".to_string());
    assert_eq!(session.id, "test-id");
    assert_eq!(session.title, "New Session");
    assert!(session.messages.is_empty());
    assert!(!session.created_at.is_empty());
    assert!(!session.updated_at.is_empty());
}

#[wasm_bindgen_test]
fn session_serialization() {
    let session = Session::new("s1".to_string());
    let json = serde_json::to_string(&session).unwrap();
    let deserialized: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "s1");
    assert_eq!(deserialized.title, "New Session");
}

#[wasm_bindgen_test]
fn session_summary_serialization() {
    let summary = SessionSummary {
        id: "s1".to_string(),
        title: "Chat about Rust".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        message_count: 5,
    };
    let json = serde_json::to_string(&summary).unwrap();
    let deserialized: SessionSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.message_count, 5);
}

// ─── Error Tests ─────────────────────────────────────────

#[wasm_bindgen_test]
fn error_display() {
    assert_eq!(AgentError::Llm("rate limit".to_string()).to_string(), "LLM error: rate limit");
    assert_eq!(AgentError::Shell("not found".to_string()).to_string(), "Shell error: not found");
    assert_eq!(AgentError::Timeout(5000).to_string(), "Timeout after 5000ms");
    assert_eq!(AgentError::Cancelled.to_string(), "Cancelled");
}

#[wasm_bindgen_test]
fn error_from_serde() {
    let bad_json = "{{invalid}}";
    let serde_err = serde_json::from_str::<serde_json::Value>(bad_json).unwrap_err();
    let agent_err: AgentError = serde_err.into();
    assert!(matches!(agent_err, AgentError::Serialization(_)));
}

#[wasm_bindgen_test]
fn error_clone() {
    let err = AgentError::Network("timeout".to_string());
    let cloned = err.clone();
    assert_eq!(err.to_string(), cloned.to_string());
}

#[wasm_bindgen_test]
fn error_fs_display() {
    let err = AgentError::Fs {
        path: "/foo".to_string(),
        message: "not found".to_string(),
    };
    assert_eq!(err.to_string(), "Filesystem error: /foo: not found");
}
