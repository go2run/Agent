//! Agent runtime — the core agent loop.
//!
//! Implements the think → act → observe cycle:
//! 1. Send messages + tool definitions to the LLM (think)
//! 2. If LLM returns tool calls, execute them (act)
//! 3. Append tool results to messages (observe)
//! 4. Loop back to step 1
//! 5. If LLM returns text only, emit the response and stop

use agent_types::{
    Result,
    config::AgentConfig,
    event::AgentEvent,
    message::{Message, ToolCallRequest},
    tool::ToolResult,
};
use crate::event_bus::EventBus;
use crate::ports::*;
use crate::tools::{ToolRegistry, parse_tool_args};

/// The agent runtime state
pub struct AgentRuntime {
    pub config: AgentConfig,
    pub messages: Vec<Message>,
    pub event_bus: EventBus,
    pub tools: ToolRegistry,
    pub state: AgentState,
    turn_counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Thinking,
    ExecutingTool { name: String, call_id: String },
    Error(String),
}

impl AgentRuntime {
    pub fn new(config: AgentConfig, event_bus: EventBus) -> Self {
        let mut messages = Vec::new();
        // Push the system prompt as the first message
        messages.push(Message::system(&config.system_prompt));

        Self {
            config,
            messages,
            event_bus,
            tools: ToolRegistry::new(),
            state: AgentState::Idle,
            turn_counter: 0,
        }
    }

    /// Run one full agent turn: user message → (think/act/observe)* → response.
    ///
    /// This is async and must be spawned via `wasm_bindgen_futures::spawn_local`.
    /// It will not block the UI thread.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        llm: &dyn LlmPort,
        shell: &dyn ShellPort,
        vfs: &dyn VfsPort,
    ) -> Result<()> {
        self.turn_counter += 1;
        let turn_id = self.turn_counter;
        self.event_bus.emit(AgentEvent::TurnStart { turn_id });

        // Add user message
        self.messages.push(Message::user(user_input));

        // Agent loop: think → act → observe → repeat
        const MAX_ITERATIONS: usize = 20;
        for _ in 0..MAX_ITERATIONS {
            self.state = AgentState::Thinking;

            // Think: call the LLM
            let req = ChatRequest {
                messages: self.messages.clone(),
                tools: self.tools.definitions(),
                model: self.config.llm.model.clone(),
                max_tokens: self.config.llm.max_tokens,
                temperature: self.config.llm.temperature,
            };

            let response = llm.chat_completion(req).await.map_err(|e| {
                self.state = AgentState::Error(e.to_string());
                self.event_bus.emit(AgentEvent::Error {
                    message: e.to_string(),
                });
                e
            })?;

            let assistant_msg = response.message;

            // Check if the assistant wants to call tools
            if assistant_msg.tool_calls.is_empty() {
                // No tool calls — final text response
                let text = assistant_msg.content.as_text().to_string();
                self.messages.push(assistant_msg);
                self.event_bus.emit(AgentEvent::LlmComplete { text });
                self.state = AgentState::Idle;
                self.event_bus.emit(AgentEvent::TurnEnd { turn_id });
                return Ok(());
            }

            // Emit the assistant's reasoning text if any
            let reasoning = assistant_msg.content.as_text().to_string();
            if !reasoning.is_empty() {
                self.event_bus.emit(AgentEvent::LlmDelta {
                    token: reasoning,
                });
            }

            let tool_calls = assistant_msg.tool_calls.clone();
            self.messages.push(assistant_msg);

            // Act: execute each tool call
            for tc in &tool_calls {
                let result = self
                    .execute_tool(tc, shell, vfs)
                    .await;

                // Observe: append tool result
                let tool_msg = Message::tool_result(
                    &tc.id,
                    &result.output,
                );
                self.messages.push(tool_msg);
            }
        }

        // Safeguard: too many iterations
        self.state = AgentState::Error("Max iterations reached".to_string());
        self.event_bus.emit(AgentEvent::Error {
            message: "Agent loop exceeded maximum iterations".to_string(),
        });
        self.event_bus.emit(AgentEvent::TurnEnd { turn_id });
        Ok(())
    }

    /// Execute a single tool call and return the result
    async fn execute_tool(
        &mut self,
        tc: &ToolCallRequest,
        shell: &dyn ShellPort,
        vfs: &dyn VfsPort,
    ) -> ToolResult {
        let call_id = tc.id.clone();
        let tool_name = tc.function.name.clone();

        self.state = AgentState::ExecutingTool {
            name: tool_name.clone(),
            call_id: call_id.clone(),
        };

        self.event_bus.emit(AgentEvent::ToolExecStart {
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: tc.function.arguments.clone(),
        });

        let args = match parse_tool_args(&tc.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                let output = format!("Failed to parse arguments: {}", e);
                self.event_bus.emit(AgentEvent::ToolExecEnd {
                    call_id: call_id.clone(),
                    result: output.clone(),
                    success: false,
                });
                return ToolResult { call_id, output, success: false };
            }
        };

        let result = match tool_name.as_str() {
            "bash" => {
                let cmd = args["command"].as_str().unwrap_or("");
                let timeout = args.get("timeout_ms").and_then(|v| v.as_u64());
                match shell.execute(cmd, timeout).await {
                    Ok(exec) => {
                        let mut output = String::new();
                        if !exec.stdout.is_empty() {
                            output.push_str(&exec.stdout);
                        }
                        if !exec.stderr.is_empty() {
                            if !output.is_empty() {
                                output.push('\n');
                            }
                            output.push_str("STDERR: ");
                            output.push_str(&exec.stderr);
                        }
                        output.push_str(&format!("\n[exit code: {}]", exec.exit_code));
                        ToolResult {
                            call_id: call_id.clone(),
                            output,
                            success: exec.exit_code == 0,
                        }
                    }
                    Err(e) => ToolResult {
                        call_id: call_id.clone(),
                        output: format!("Shell error: {}", e),
                        success: false,
                    },
                }
            }
            "read_file" => {
                let path = args["path"].as_str().unwrap_or("");
                match vfs.read_file(path).await {
                    Ok(data) => {
                        let text = String::from_utf8_lossy(&data).to_string();
                        ToolResult {
                            call_id: call_id.clone(),
                            output: text,
                            success: true,
                        }
                    }
                    Err(e) => ToolResult {
                        call_id: call_id.clone(),
                        output: format!("Read error: {}", e),
                        success: false,
                    },
                }
            }
            "write_file" => {
                let path = args["path"].as_str().unwrap_or("");
                let content = args["content"].as_str().unwrap_or("");
                match vfs.write_file(path, content.as_bytes()).await {
                    Ok(()) => ToolResult {
                        call_id: call_id.clone(),
                        output: format!("Written {} bytes to {}", content.len(), path),
                        success: true,
                    },
                    Err(e) => ToolResult {
                        call_id: call_id.clone(),
                        output: format!("Write error: {}", e),
                        success: false,
                    },
                }
            }
            "list_dir" => {
                let path = args["path"].as_str().unwrap_or("/");
                match vfs.list_dir(path).await {
                    Ok(entries) => {
                        let listing: Vec<String> = entries.iter().map(|e| {
                            let prefix = if e.is_dir { "d " } else { "- " };
                            format!("{}{:>8}  {}", prefix, e.size, e.name)
                        }).collect();
                        ToolResult {
                            call_id: call_id.clone(),
                            output: listing.join("\n"),
                            success: true,
                        }
                    }
                    Err(e) => ToolResult {
                        call_id: call_id.clone(),
                        output: format!("List error: {}", e),
                        success: false,
                    },
                }
            }
            _ => ToolResult {
                call_id: call_id.clone(),
                output: format!("Unknown tool: {}", tool_name),
                success: false,
            },
        };

        self.event_bus.emit(AgentEvent::ToolExecEnd {
            call_id: result.call_id.clone(),
            result: result.output.clone(),
            success: result.success,
        });

        result
    }

    /// Reset the conversation (keep system prompt)
    pub fn reset(&mut self) {
        self.messages.truncate(1); // keep system prompt
        self.state = AgentState::Idle;
        self.turn_counter = 0;
    }
}
