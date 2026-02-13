//! Shared types and helpers for OpenAI-compatible chat APIs (Venice, Gab, etc.)

use color_eyre::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::time::Duration;

use crate::agents::{ChatMessage, MessageRole};

// -- Tool calling types --

/// A tool definition sent in the request to enable native function calling
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// The function schema within a tool definition
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

/// A tool call returned by the model in its response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallResponse {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub call_type: String,
    pub function: FunctionCallResponse,
}

/// The function name and arguments within a tool call response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionCallResponse {
    pub name: String,
    /// JSON-encoded arguments string
    pub arguments: String,
}

/// Unified chat response that includes both content and optional tool calls
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCallResponse>,
}

impl ChatResponse {
    /// Creates a simple text-only response (no tool calls)
    pub fn text(content: String) -> Self {
        Self {
            content,
            tool_calls: Vec::new(),
        }
    }

    /// Returns true if the model requested tool calls
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

// -- Shared request/response types --

#[derive(Debug, Serialize)]
pub struct OpenAIChatRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls made by the assistant (present when role = "assistant")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
    /// The ID of the tool call this message is responding to (present when role = "tool")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIChatResponse {
    pub choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIChoice {
    pub message: OpenAIChoiceMessage,
}

/// The message inside a choice -- separate from OpenAIMessage to handle nullable content
#[derive(Debug, Deserialize)]
pub struct OpenAIChoiceMessage {
    #[allow(dead_code)]
    pub role: String,
    /// Content may be null when the model only makes tool calls
    pub content: Option<String>,
    /// Tool calls requested by the model
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
}

// -- Conversion helpers --

/// Converts internal `ChatMessage` list to OpenAI-compatible messages
pub fn convert_messages(messages: &[ChatMessage]) -> Vec<OpenAIMessage> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };

            OpenAIMessage {
                role: role.to_string(),
                content: Some(msg.content.clone()),
                tool_calls: msg.tool_calls.clone(),
                tool_call_id: msg.tool_call_id.clone(),
            }
        })
        .collect()
}

/// Extracts the assistant reply from an OpenAI-style response
pub fn extract_reply(response: OpenAIChatResponse, provider: &str) -> Result<String> {
    response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| color_eyre::eyre::eyre!("{} response missing content", provider))
}

/// Extracts a full ChatResponse (content + tool_calls) from an OpenAI-style response
pub fn extract_chat_response(response: OpenAIChatResponse, provider: &str) -> Result<ChatResponse> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| color_eyre::eyre::eyre!("{} response missing choices", provider))?;

    Ok(ChatResponse {
        content: choice.message.content.unwrap_or_default(),
        tool_calls: choice.message.tool_calls.unwrap_or_default(),
    })
}

/// Builds a `reqwest::blocking::Client` with standard timeouts
pub fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()?)
}
