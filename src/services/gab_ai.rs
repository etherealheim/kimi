use color_eyre::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_GAB_BASE_URL: &str = "https://api.gab.ai/v1";

#[derive(Debug, Deserialize)]
struct GabChatResponse {
    choices: Vec<GabChoice>,
}

#[derive(Debug, Deserialize)]
struct GabChoice {
    message: GabMessage,
}

#[derive(Debug, Deserialize)]
struct GabMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct GabChatRequest {
    model: String,
    messages: Vec<GabChatMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct GabChatMessage {
    role: String,
    content: String,
}

pub fn default_base_url() -> String {
    DEFAULT_GAB_BASE_URL.to_string()
}

pub fn chat(
    api_key: &str,
    base_url: &str,
    model: &str,
    messages: &[crate::agents::ChatMessage],
) -> Result<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let chat_messages = messages
        .iter()
        .map(|msg| GabChatMessage {
            role: match msg.role {
                crate::agents::MessageRole::System => "system".to_string(),
                crate::agents::MessageRole::User => "user".to_string(),
                crate::agents::MessageRole::Assistant => "assistant".to_string(),
            },
            content: msg.content.clone(),
        })
        .collect();

    let request = GabChatRequest {
        model: model.to_string(),
        messages: chat_messages,
        stream: false,
    };

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()?;
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&request)
        .send()?
        .error_for_status()?;
    let payload: GabChatResponse = response.json()?;
    payload
        .choices
        .get(0)
        .map(|choice| choice.message.content.clone())
        .ok_or_else(|| color_eyre::eyre::eyre!("Gab AI response missing content"))
}
