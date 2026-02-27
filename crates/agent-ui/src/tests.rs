#[cfg(test)]
mod tests {
    use crate::state::*;
    use agent_types::event::AgentEvent;
    use agent_core::runtime::AgentState;

    // ─── UiState Tests ───────────────────────────────────────

    #[test]
    fn test_ui_state_initial() {
        let state = UiState::new();
        assert!(state.messages.is_empty());
        assert_eq!(state.agent_status, AgentState::Idle);
        assert!(state.terminal_lines.is_empty());
        assert!(state.streaming_text.is_empty());
        assert!(state.input_text.is_empty());
        assert!(!state.show_settings);
        assert_eq!(state.status_text, "Ready");
        assert!(!state.is_busy());
    }

    #[test]
    fn test_ui_state_push_user_message() {
        let mut state = UiState::new();
        state.push_user_message("hello");
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[0].content, "hello");
        assert!(!state.messages[0].is_tool_call);
        assert!(state.messages[0].tool_name.is_none());
    }

    #[test]
    fn test_ui_state_process_turn_start() {
        let mut state = UiState::new();
        state.process_events(vec![AgentEvent::TurnStart { turn_id: 1 }]);

        assert_eq!(state.agent_status, AgentState::Thinking);
        assert!(state.streaming_text.is_empty());
        assert_eq!(state.status_text, "Thinking...");
        assert!(state.is_busy());
    }

    #[test]
    fn test_ui_state_process_llm_delta() {
        let mut state = UiState::new();
        state.process_events(vec![
            AgentEvent::LlmDelta { token: "Hello".to_string() },
            AgentEvent::LlmDelta { token: " world".to_string() },
        ]);
        assert_eq!(state.streaming_text, "Hello world");
    }

    #[test]
    fn test_ui_state_process_llm_complete() {
        let mut state = UiState::new();
        state.streaming_text = "partial".to_string();
        state.process_events(vec![AgentEvent::LlmComplete {
            text: "Full response".to_string(),
        }]);

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "assistant");
        assert_eq!(state.messages[0].content, "Full response");
        assert!(state.streaming_text.is_empty());
    }

    #[test]
    fn test_ui_state_process_tool_exec() {
        let mut state = UiState::new();

        state.process_events(vec![AgentEvent::ToolExecStart {
            call_id: "c1".to_string(),
            tool_name: "bash".to_string(),
            arguments: r#"{"command":"ls"}"#.to_string(),
        }]);

        assert_eq!(state.status_text, "Running: bash");
        assert_eq!(state.terminal_lines.len(), 1);
        assert!(state.terminal_lines[0].text.contains("bash"));
    }

    #[test]
    fn test_ui_state_process_tool_output() {
        let mut state = UiState::new();

        state.process_events(vec![
            AgentEvent::ToolOutput {
                call_id: "c1".to_string(),
                chunk: "file1.txt".to_string(),
            },
            AgentEvent::ToolOutput {
                call_id: "c1".to_string(),
                chunk: "file2.txt".to_string(),
            },
        ]);

        assert_eq!(state.terminal_lines.len(), 2);
        assert_eq!(state.terminal_lines[0].text, "file1.txt");
        assert_eq!(state.terminal_lines[1].text, "file2.txt");
        assert!(!state.terminal_lines[0].is_stderr);
    }

    #[test]
    fn test_ui_state_process_tool_exec_end() {
        let mut state = UiState::new();

        state.process_events(vec![AgentEvent::ToolExecEnd {
            call_id: "c1".to_string(),
            result: "output here".to_string(),
            success: true,
        }]);

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "tool");
        assert_eq!(state.messages[0].content, "output here");
        assert!(state.messages[0].is_tool_call);
    }

    #[test]
    fn test_ui_state_process_turn_end() {
        let mut state = UiState::new();
        state.agent_status = AgentState::Thinking;

        state.process_events(vec![AgentEvent::TurnEnd { turn_id: 1 }]);

        assert_eq!(state.agent_status, AgentState::Idle);
        assert_eq!(state.status_text, "Ready");
        assert!(!state.is_busy());
    }

    #[test]
    fn test_ui_state_process_error() {
        let mut state = UiState::new();

        state.process_events(vec![AgentEvent::Error {
            message: "API error".to_string(),
        }]);

        assert!(matches!(state.agent_status, AgentState::Error(_)));
        assert!(state.status_text.contains("API error"));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "error");
        assert!(!state.is_busy()); // Error state is not "busy"
    }

    #[test]
    fn test_ui_state_full_turn_lifecycle() {
        let mut state = UiState::new();

        // Simulate a complete turn
        state.push_user_message("run ls");

        state.process_events(vec![
            AgentEvent::TurnStart { turn_id: 1 },
        ]);
        assert!(state.is_busy());

        state.process_events(vec![
            AgentEvent::ToolExecStart {
                call_id: "c1".to_string(),
                tool_name: "bash".to_string(),
                arguments: r#"{"command":"ls"}"#.to_string(),
            },
        ]);

        state.process_events(vec![
            AgentEvent::ToolOutput {
                call_id: "c1".to_string(),
                chunk: "file1.txt\nfile2.txt".to_string(),
            },
        ]);

        state.process_events(vec![
            AgentEvent::ToolExecEnd {
                call_id: "c1".to_string(),
                result: "file1.txt\nfile2.txt".to_string(),
                success: true,
            },
        ]);

        state.process_events(vec![
            AgentEvent::LlmComplete {
                text: "Here are the files in the directory.".to_string(),
            },
        ]);

        state.process_events(vec![
            AgentEvent::TurnEnd { turn_id: 1 },
        ]);

        assert!(!state.is_busy());
        assert_eq!(state.status_text, "Ready");
        // user + tool_result + assistant = 3 messages
        assert_eq!(state.messages.len(), 3);
        assert!(state.terminal_lines.len() >= 1);
    }

    #[test]
    fn test_ui_state_is_busy_states() {
        let mut state = UiState::new();

        state.agent_status = AgentState::Idle;
        assert!(!state.is_busy());

        state.agent_status = AgentState::Thinking;
        assert!(state.is_busy());

        state.agent_status = AgentState::ExecutingTool {
            name: "bash".to_string(),
            call_id: "c1".to_string(),
        };
        assert!(state.is_busy());

        state.agent_status = AgentState::Error("err".to_string());
        assert!(!state.is_busy());
    }

    #[test]
    fn test_ui_state_default() {
        let state = UiState::default();
        assert!(state.messages.is_empty());
        assert!(!state.is_busy());
    }
}
