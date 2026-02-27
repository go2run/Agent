//! UI-level state that drives rendering.
//! This is a read-only projection of the agent runtime state,
//! updated each frame by draining the EventBus.

use agent_types::event::AgentEvent;
use agent_core::runtime::AgentState;

/// State visible to UI panels
pub struct UiState {
    /// Displayed messages (user + assistant + tool results)
    pub messages: Vec<ChatEntry>,
    /// Current agent status
    pub agent_status: AgentState,
    /// Terminal output buffer (from bash executions)
    pub terminal_lines: Vec<TerminalLine>,
    /// Streaming LLM text being assembled
    pub streaming_text: String,
    /// Input field content
    pub input_text: String,
    /// Whether settings panel is open
    pub show_settings: bool,
    /// Status line text
    pub status_text: String,
}

/// A chat entry for display
#[derive(Clone)]
pub struct ChatEntry {
    pub role: String,
    pub content: String,
    pub is_tool_call: bool,
    pub tool_name: Option<String>,
}

/// A line in the terminal output
#[derive(Clone)]
pub struct TerminalLine {
    pub text: String,
    pub is_stderr: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            agent_status: AgentState::Idle,
            terminal_lines: Vec::new(),
            streaming_text: String::new(),
            input_text: String::new(),
            show_settings: false,
            status_text: "Ready".to_string(),
        }
    }

    /// Process events from the EventBus and update UI state
    pub fn process_events(&mut self, events: Vec<AgentEvent>) {
        for event in events {
            match event {
                AgentEvent::TurnStart { .. } => {
                    self.agent_status = AgentState::Thinking;
                    self.streaming_text.clear();
                    self.status_text = "Thinking...".to_string();
                }
                AgentEvent::LlmDelta { token } => {
                    self.streaming_text.push_str(&token);
                }
                AgentEvent::LlmComplete { text } => {
                    self.messages.push(ChatEntry {
                        role: "assistant".to_string(),
                        content: text,
                        is_tool_call: false,
                        tool_name: None,
                    });
                    self.streaming_text.clear();
                }
                AgentEvent::ToolExecStart {
                    tool_name,
                    arguments,
                    ..
                } => {
                    self.status_text = format!("Running: {}", tool_name);
                    self.terminal_lines.push(TerminalLine {
                        text: format!("$ {} {}", tool_name, arguments),
                        is_stderr: false,
                    });
                }
                AgentEvent::ToolOutput { chunk, .. } => {
                    self.terminal_lines.push(TerminalLine {
                        text: chunk,
                        is_stderr: false,
                    });
                }
                AgentEvent::ToolExecEnd {
                    call_id,
                    result,
                    ..
                } => {
                    self.messages.push(ChatEntry {
                        role: "tool".to_string(),
                        content: result,
                        is_tool_call: true,
                        tool_name: Some(call_id),
                    });
                }
                AgentEvent::TurnEnd { .. } => {
                    self.agent_status = AgentState::Idle;
                    self.status_text = "Ready".to_string();
                }
                AgentEvent::Error { message } => {
                    self.agent_status = AgentState::Error(message.clone());
                    self.status_text = format!("Error: {}", message);
                    self.messages.push(ChatEntry {
                        role: "error".to_string(),
                        content: message,
                        is_tool_call: false,
                        tool_name: None,
                    });
                }
            }
        }
    }

    /// Add a user message to the display
    pub fn push_user_message(&mut self, text: &str) {
        self.messages.push(ChatEntry {
            role: "user".to_string(),
            content: text.to_string(),
            is_tool_call: false,
            tool_name: None,
        });
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.agent_status, AgentState::Idle | AgentState::Error(_))
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}
