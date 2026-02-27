#[cfg(test)]
mod tests {
    use crate::event_bus::EventBus;
    use crate::tools::{ToolRegistry, parse_tool_args};
    use crate::runtime::{AgentRuntime, AgentState};
    use crate::ports::*;
    use agent_types::config::AgentConfig;
    use agent_types::event::AgentEvent;
    use agent_types::message::*;
    use agent_types::tool::*;
    use std::pin::Pin;
    use async_trait::async_trait;
    use futures::Stream;

    // ─── EventBus Tests ──────────────────────────────────────

    #[test]
    fn test_event_bus_new_is_empty() {
        let bus = EventBus::new();
        assert!(!bus.has_pending());
        assert!(bus.drain().is_empty());
    }

    #[test]
    fn test_event_bus_emit_and_drain() {
        let bus = EventBus::new();
        bus.emit(AgentEvent::TurnStart { turn_id: 1 });
        bus.emit(AgentEvent::LlmComplete { text: "hello".to_string() });

        assert!(bus.has_pending());

        let events = bus.drain();
        assert_eq!(events.len(), 2);
        assert!(!bus.has_pending());
    }

    #[test]
    fn test_event_bus_drain_empties() {
        let bus = EventBus::new();
        bus.emit(AgentEvent::TurnStart { turn_id: 1 });
        let _ = bus.drain();
        assert!(bus.drain().is_empty());
    }

    #[test]
    fn test_event_bus_clone_shares_state() {
        let bus1 = EventBus::new();
        let bus2 = bus1.clone();

        bus1.emit(AgentEvent::TurnStart { turn_id: 1 });
        assert!(bus2.has_pending());

        let events = bus2.drain();
        assert_eq!(events.len(), 1);
        assert!(!bus1.has_pending());
    }

    #[test]
    fn test_event_bus_multiple_emits() {
        let bus = EventBus::new();
        for i in 0..100 {
            bus.emit(AgentEvent::LlmDelta { token: format!("tok{}", i) });
        }
        let events = bus.drain();
        assert_eq!(events.len(), 100);
    }

    // ─── ToolRegistry Tests ──────────────────────────────────

    #[test]
    fn test_tool_registry_has_builtins() {
        let registry = ToolRegistry::new();
        let defs = registry.definitions();
        assert!(defs.len() >= 4, "Expected at least 4 built-in tools, got {}", defs.len());

        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"bash"), "Missing bash tool");
        assert!(names.contains(&"read_file"), "Missing read_file tool");
        assert!(names.contains(&"write_file"), "Missing write_file tool");
        assert!(names.contains(&"list_dir"), "Missing list_dir tool");
    }

    #[test]
    fn test_tool_registry_get() {
        let registry = ToolRegistry::new();
        let bash = registry.get("bash");
        assert!(bash.is_some());
        assert_eq!(bash.unwrap().name, "bash");
        assert!(!bash.unwrap().description.is_empty());
    }

    #[test]
    fn test_tool_registry_get_missing() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_tool_parameters_schema() {
        let registry = ToolRegistry::new();
        let bash = registry.get("bash").unwrap();
        assert_eq!(bash.parameters.schema_type, "object");
        assert!(bash.parameters.required.contains(&"command".to_string()));
        assert!(bash.parameters.properties.contains_key("command"));
    }

    #[test]
    fn test_tool_definitions_are_valid_json() {
        let registry = ToolRegistry::new();
        for tool in registry.definitions() {
            let json = serde_json::to_string(&tool).unwrap();
            let _: serde_json::Value = serde_json::from_str(&json).unwrap();
        }
    }

    // ─── parse_tool_args Tests ───────────────────────────────

    #[test]
    fn test_parse_tool_args_valid() {
        let args = parse_tool_args(r#"{"command": "ls -la"}"#).unwrap();
        assert_eq!(args["command"].as_str().unwrap(), "ls -la");
    }

    #[test]
    fn test_parse_tool_args_multiple_fields() {
        let args = parse_tool_args(r#"{"path": "/home", "content": "hello"}"#).unwrap();
        assert_eq!(args["path"].as_str().unwrap(), "/home");
        assert_eq!(args["content"].as_str().unwrap(), "hello");
    }

    #[test]
    fn test_parse_tool_args_invalid_json() {
        let result = parse_tool_args("{{not json}}");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tool_args_empty_object() {
        let args = parse_tool_args("{}").unwrap();
        assert!(args.as_object().unwrap().is_empty());
    }

    // ─── AgentRuntime Tests ──────────────────────────────────

    #[test]
    fn test_runtime_initial_state() {
        let config = AgentConfig::default();
        let bus = EventBus::new();
        let runtime = AgentRuntime::new(config, bus);
        assert_eq!(runtime.state, AgentState::Idle);
        // Should have system prompt as first message
        assert_eq!(runtime.messages.len(), 1);
        assert_eq!(runtime.messages[0].role, Role::System);
    }

    #[test]
    fn test_runtime_reset() {
        let config = AgentConfig::default();
        let bus = EventBus::new();
        let mut runtime = AgentRuntime::new(config, bus);

        // Add some messages
        runtime.messages.push(Message::user("hello"));
        runtime.messages.push(Message::assistant("hi"));
        assert_eq!(runtime.messages.len(), 3);

        runtime.reset();
        assert_eq!(runtime.messages.len(), 1); // only system prompt
        assert_eq!(runtime.state, AgentState::Idle);
    }

    #[test]
    fn test_agent_state_eq() {
        assert_eq!(AgentState::Idle, AgentState::Idle);
        assert_eq!(AgentState::Thinking, AgentState::Thinking);
        assert_ne!(AgentState::Idle, AgentState::Thinking);
    }

    // ─── Mock-based Agent Loop Test ──────────────────────────

    /// Mock LLM that returns a simple text response (no tool calls)
    struct MockLlm {
        response_text: String,
    }

    #[async_trait(?Send)]
    impl LlmPort for MockLlm {
        async fn chat_completion(&self, _req: ChatRequest) -> agent_types::Result<ChatResponse> {
            Ok(ChatResponse {
                message: Message::assistant(&self.response_text),
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }),
            })
        }

        fn stream_chat(
            &self,
            _req: ChatRequest,
        ) -> Pin<Box<dyn Stream<Item = LlmStreamEvent>>> {
            Box::pin(futures::stream::once(async { LlmStreamEvent::Done }))
        }

        async fn list_models(&self) -> agent_types::Result<Vec<String>> {
            Ok(vec!["mock-model".to_string()])
        }
    }

    /// Mock LLM that returns a tool call, then a text response
    struct MockLlmWithToolCall {
        call_count: std::cell::RefCell<usize>,
    }

    #[async_trait(?Send)]
    impl LlmPort for MockLlmWithToolCall {
        async fn chat_completion(&self, _req: ChatRequest) -> agent_types::Result<ChatResponse> {
            let mut count = self.call_count.borrow_mut();
            *count += 1;

            if *count == 1 {
                // First call: return a tool call
                Ok(ChatResponse {
                    message: Message {
                        role: Role::Assistant,
                        content: MessageContent::Text("Let me check".to_string()),
                        tool_call_id: None,
                        tool_calls: vec![ToolCallRequest {
                            id: "call_1".to_string(),
                            function: FunctionCall {
                                name: "bash".to_string(),
                                arguments: r#"{"command":"echo test"}"#.to_string(),
                            },
                        }],
                    },
                    usage: None,
                })
            } else {
                // Second call: return text response
                Ok(ChatResponse {
                    message: Message::assistant("Done! The command ran successfully."),
                    usage: None,
                })
            }
        }

        fn stream_chat(
            &self,
            _req: ChatRequest,
        ) -> Pin<Box<dyn Stream<Item = LlmStreamEvent>>> {
            Box::pin(futures::stream::once(async { LlmStreamEvent::Done }))
        }

        async fn list_models(&self) -> agent_types::Result<Vec<String>> {
            Ok(vec![])
        }
    }

    /// Mock shell that returns fixed output
    struct MockShell;

    #[async_trait(?Send)]
    impl ShellPort for MockShell {
        async fn execute(&self, cmd: &str, _timeout_ms: Option<u64>) -> agent_types::Result<ExecResult> {
            Ok(ExecResult {
                stdout: format!("mock output for: {}", cmd),
                stderr: String::new(),
                exit_code: 0,
            })
        }

        fn execute_streaming(
            &self,
            _cmd: &str,
        ) -> Pin<Box<dyn Stream<Item = ShellStreamEvent>>> {
            Box::pin(futures::stream::empty())
        }

        async fn cancel(&self, _handle: ExecHandle) -> agent_types::Result<()> {
            Ok(())
        }

        fn is_ready(&self) -> bool {
            true
        }
    }

    /// Mock VFS
    struct MockVfs {
        files: std::cell::RefCell<std::collections::HashMap<String, Vec<u8>>>,
    }

    impl MockVfs {
        fn new() -> Self {
            Self {
                files: std::cell::RefCell::new(std::collections::HashMap::new()),
            }
        }
    }

    #[async_trait(?Send)]
    impl VfsPort for MockVfs {
        async fn read_file(&self, path: &str) -> agent_types::Result<Vec<u8>> {
            self.files
                .borrow()
                .get(path)
                .cloned()
                .ok_or_else(|| agent_types::AgentError::Fs {
                    path: path.to_string(),
                    message: "not found".to_string(),
                })
        }

        async fn write_file(&self, path: &str, data: &[u8]) -> agent_types::Result<()> {
            self.files.borrow_mut().insert(path.to_string(), data.to_vec());
            Ok(())
        }

        async fn delete_file(&self, path: &str) -> agent_types::Result<()> {
            self.files.borrow_mut().remove(path);
            Ok(())
        }

        async fn list_dir(&self, _path: &str) -> agent_types::Result<Vec<DirEntry>> {
            Ok(vec![DirEntry {
                name: "test.txt".to_string(),
                is_dir: false,
                size: 100,
            }])
        }

        async fn stat(&self, path: &str) -> agent_types::Result<FileStat> {
            if self.files.borrow().contains_key(path) {
                Ok(FileStat {
                    size: self.files.borrow()[path].len() as u64,
                    is_dir: false,
                    modified: None,
                })
            } else {
                Err(agent_types::AgentError::Fs {
                    path: path.to_string(),
                    message: "not found".to_string(),
                })
            }
        }

        async fn mkdir(&self, _path: &str) -> agent_types::Result<()> {
            Ok(())
        }

        async fn exists(&self, path: &str) -> agent_types::Result<bool> {
            Ok(self.files.borrow().contains_key(path))
        }
    }

    // Use tokio-like block_on for sync tests (since we're not in WASM here)
    fn block_on<F: std::future::Future<Output = T>, T>(f: F) -> T {
        // Simple futures executor for single-threaded tests
        use std::task::{Context, Poll, Wake, Waker};
        use std::sync::Arc;

        struct NoopWaker;
        impl Wake for NoopWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(NoopWaker));
        let mut cx = Context::from_waker(&waker);
        let mut f = std::pin::pin!(f);

        loop {
            match f.as_mut().poll(&mut cx) {
                Poll::Ready(val) => return val,
                Poll::Pending => {
                    // For our mock tests, everything completes immediately
                    // so this shouldn't actually spin
                    std::thread::yield_now();
                }
            }
        }
    }

    #[test]
    fn test_agent_loop_simple_response() {
        let bus = EventBus::new();
        let config = AgentConfig::default();
        let mut runtime = AgentRuntime::new(config, bus.clone());

        let llm = MockLlm {
            response_text: "Hello, I'm your agent!".to_string(),
        };
        let shell = MockShell;
        let vfs = MockVfs::new();

        block_on(runtime.run_turn("Hi", &llm, &shell, &vfs)).unwrap();

        // Should have: system + user + assistant = 3 messages
        assert_eq!(runtime.messages.len(), 3);
        assert_eq!(runtime.messages[1].role, Role::User);
        assert_eq!(runtime.messages[1].content.as_text(), "Hi");
        assert_eq!(runtime.messages[2].role, Role::Assistant);
        assert_eq!(runtime.messages[2].content.as_text(), "Hello, I'm your agent!");
        assert_eq!(runtime.state, AgentState::Idle);

        // Check events
        let events = bus.drain();
        assert!(events.len() >= 2); // TurnStart + LlmComplete + TurnEnd
    }

    #[test]
    fn test_agent_loop_with_tool_call() {
        let bus = EventBus::new();
        let config = AgentConfig::default();
        let mut runtime = AgentRuntime::new(config, bus.clone());

        let llm = MockLlmWithToolCall {
            call_count: std::cell::RefCell::new(0),
        };
        let shell = MockShell;
        let vfs = MockVfs::new();

        block_on(runtime.run_turn("Run ls", &llm, &shell, &vfs)).unwrap();

        // system + user + assistant(tool_call) + tool_result + assistant(final) = 5
        assert_eq!(runtime.messages.len(), 5);
        assert_eq!(runtime.messages[2].role, Role::Assistant);
        assert!(!runtime.messages[2].tool_calls.is_empty());
        assert_eq!(runtime.messages[3].role, Role::Tool);
        assert_eq!(runtime.messages[4].role, Role::Assistant);
        assert_eq!(runtime.state, AgentState::Idle);

        // Check events include tool execution
        let events = bus.drain();
        let has_tool_start = events.iter().any(|e| matches!(e, AgentEvent::ToolExecStart { .. }));
        let has_tool_end = events.iter().any(|e| matches!(e, AgentEvent::ToolExecEnd { .. }));
        assert!(has_tool_start, "Missing ToolExecStart event");
        assert!(has_tool_end, "Missing ToolExecEnd event");
    }

    #[test]
    fn test_agent_loop_multiple_turns() {
        let bus = EventBus::new();
        let config = AgentConfig::default();
        let mut runtime = AgentRuntime::new(config, bus.clone());

        let llm = MockLlm {
            response_text: "Response".to_string(),
        };
        let shell = MockShell;
        let vfs = MockVfs::new();

        block_on(runtime.run_turn("Turn 1", &llm, &shell, &vfs)).unwrap();
        let _ = bus.drain();
        block_on(runtime.run_turn("Turn 2", &llm, &shell, &vfs)).unwrap();

        // system + (user+assistant)*2 = 5
        assert_eq!(runtime.messages.len(), 5);
    }

    /// Mock LLM that returns an error
    struct MockLlmError;

    #[async_trait(?Send)]
    impl LlmPort for MockLlmError {
        async fn chat_completion(&self, _req: ChatRequest) -> agent_types::Result<ChatResponse> {
            Err(agent_types::AgentError::Llm("API key invalid".to_string()))
        }

        fn stream_chat(
            &self,
            _req: ChatRequest,
        ) -> Pin<Box<dyn Stream<Item = LlmStreamEvent>>> {
            Box::pin(futures::stream::once(async { LlmStreamEvent::Done }))
        }

        async fn list_models(&self) -> agent_types::Result<Vec<String>> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_agent_loop_llm_error() {
        let bus = EventBus::new();
        let config = AgentConfig::default();
        let mut runtime = AgentRuntime::new(config, bus.clone());

        let llm = MockLlmError;
        let shell = MockShell;
        let vfs = MockVfs::new();

        let result = block_on(runtime.run_turn("Hi", &llm, &shell, &vfs));
        assert!(result.is_err());

        // Check error event was emitted
        let events = bus.drain();
        let has_error = events.iter().any(|e| matches!(e, AgentEvent::Error { .. }));
        assert!(has_error, "Missing Error event");
    }

    // ─── Mock VFS Operation Tests ────────────────────────────

    #[test]
    fn test_mock_vfs_write_and_read() {
        let vfs = MockVfs::new();
        block_on(async {
            vfs.write_file("/test.txt", b"hello world").await.unwrap();
            let data = vfs.read_file("/test.txt").await.unwrap();
            assert_eq!(data, b"hello world");
        });
    }

    #[test]
    fn test_mock_vfs_read_nonexistent() {
        let vfs = MockVfs::new();
        block_on(async {
            let result = vfs.read_file("/nonexistent").await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_mock_vfs_delete() {
        let vfs = MockVfs::new();
        block_on(async {
            vfs.write_file("/test.txt", b"data").await.unwrap();
            assert!(vfs.exists("/test.txt").await.unwrap());
            vfs.delete_file("/test.txt").await.unwrap();
            assert!(!vfs.exists("/test.txt").await.unwrap());
        });
    }
}
