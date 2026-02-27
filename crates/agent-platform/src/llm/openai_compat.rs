//! OpenAI-compatible LLM adapter.
//!
//! Works with DeepSeek, OpenAI, and any provider using the
//! OpenAI chat completions API format.
//! Uses browser `fetch()` via gloo-net for WASM compatibility.

use std::pin::Pin;
use async_trait::async_trait;
use futures::stream::{self, Stream};
use gloo_net::http::Request;
use serde::Deserialize;
use serde_json::{json, Value};

use agent_core::ports::*;
use agent_types::{
    Result, AgentError,
    config::LlmConfig,
    message::{Message, MessageContent, Role, ToolCallRequest, FunctionCall},
};

/// Provider that speaks the OpenAI chat completions protocol.
/// Compatible with: DeepSeek, OpenAI, Groq, Together, Mistral, etc.
pub struct OpenAiCompatProvider {
    config: LlmConfig,
    base_url: String,
}

impl OpenAiCompatProvider {
    pub fn new(config: LlmConfig) -> Self {
        let base_url = config
            .api_base
            .clone()
            .unwrap_or_else(|| config.provider.default_base_url().to_string());
        Self { config, base_url }
    }

    fn build_request_body(&self, req: &ChatRequest) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| message_to_json(m))
            .collect();

        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
        });

        if !req.tools.is_empty() {
            let tools: Vec<Value> = req
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools);
        }

        body
    }
}

#[async_trait(?Send)]
impl LlmPort for OpenAiCompatProvider {
    async fn chat_completion(&self, req: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = self.build_request_body(&req);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", self.config.api_key))
            .json(&body)
            .map_err(|e| AgentError::Network(e.to_string()))?
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;

        if !response.ok() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AgentError::Llm(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let data: ApiResponse = response
            .json()
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::Llm("No choices in response".to_string()))?;

        let message = parse_api_message(choice.message);
        let usage = data.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(ChatResponse { message, usage })
    }

    fn stream_chat(
        &self,
        _req: ChatRequest,
    ) -> Pin<Box<dyn Stream<Item = LlmStreamEvent>>> {
        // Streaming via SSE requires ReadableStream parsing.
        // For now, return a placeholder that yields Done immediately.
        // Full SSE streaming will be implemented in a follow-up.
        Box::pin(stream::once(async { LlmStreamEvent::Done }))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/models", self.base_url);

        let response = Request::get(&url)
            .header("Authorization", &format!("Bearer {}", self.config.api_key))
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;

        if !response.ok() {
            return Err(AgentError::Llm(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let models = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

// ─── API response types ──────────────────────────────────────

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: ApiMessage,
}

#[derive(Deserialize)]
struct ApiMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCall>,
}

#[derive(Deserialize)]
struct ApiToolCall {
    id: String,
    function: ApiFunction,
}

#[derive(Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ─── Serialization helpers ───────────────────────────────────

fn message_to_json(msg: &Message) -> Value {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };

    let mut obj = json!({
        "role": role,
        "content": msg.content.as_text(),
    });

    if let Some(ref id) = msg.tool_call_id {
        obj["tool_call_id"] = json!(id);
    }

    if !msg.tool_calls.is_empty() {
        let calls: Vec<Value> = msg
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.function.name,
                        "arguments": tc.function.arguments,
                    }
                })
            })
            .collect();
        obj["tool_calls"] = json!(calls);
    }

    obj
}

fn parse_api_message(api: ApiMessage) -> Message {
    let role = match api.role.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    };

    let content = MessageContent::Text(api.content.unwrap_or_default());

    let tool_calls: Vec<ToolCallRequest> = api
        .tool_calls
        .into_iter()
        .map(|tc| ToolCallRequest {
            id: tc.id,
            function: FunctionCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            },
        })
        .collect();

    Message {
        role,
        content,
        tool_call_id: None,
        tool_calls,
    }
}
