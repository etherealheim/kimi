use color_eyre::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_GAB_BASE_URL: &str = "https://gab.ai/v1";

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
    let model = model.to_lowercase();
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
        model,
        messages: chat_messages,
        stream: false,
    };

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut last_error: Option<color_eyre::Report> = None;
    for base in gab_base_candidates(base_url) {
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let response = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&request)
            .send();
        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let payload: GabChatResponse = response.json()?;
                    return payload
                        .choices
                        .get(0)
                        .map(|choice| choice.message.content.clone())
                        .ok_or_else(|| color_eyre::eyre::eyre!("Gab AI response missing content"));
                }
                let details = response.text().unwrap_or_default();
                if status.as_u16() == 404 || status.as_u16() == 405 {
                    last_error = Some(color_eyre::eyre::eyre!(
                        "Gab AI endpoint not found ({}): {}",
                        status,
                        url
                    ));
                    continue;
                }
                return Err(color_eyre::eyre::eyre!(
                    "Gab AI error: {} {}",
                    status,
                    details
                ));
            }
            Err(error) => {
                last_error = Some(color_eyre::eyre::eyre!(
                    "Gab AI request error ({}): {}",
                    url,
                    error
                ));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        color_eyre::eyre::eyre!("Gab AI request failed")
    }))
}

fn gab_base_candidates(base_url: &str) -> Vec<String> {
    let trimmed = base_url.trim_end_matches('/').to_string();
    let normalized = normalize_gab_base(&trimmed);
    let mut candidates = Vec::new();
    let mut push = |value: String| {
        if !candidates.contains(&value) {
            candidates.push(value);
        }
    };
    push(normalized.clone());
    if let Some(swapped) = swap_gab_host(&normalized) {
        push(swapped);
    }
    push(format!("{}/v1", normalized));
    if let Some(swapped) = swap_gab_host(&normalized) {
        push(format!("{}/v1", swapped));
    }
    push(format!("{}/api/v1", normalized));
    if let Some(swapped) = swap_gab_host(&normalized) {
        push(format!("{}/api/v1", swapped));
    }
    candidates
}

fn normalize_gab_base(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/').to_string();
    let trimmed = trimmed.replace("https://api.gab.ai", "https://gab.ai");
    if let Some(stripped) = trimmed.strip_suffix("/api/v1") {
        return stripped.to_string();
    }
    if let Some(stripped) = trimmed.strip_suffix("/v1") {
        return stripped.to_string();
    }
    trimmed
}

fn swap_gab_host(base_url: &str) -> Option<String> {
    if base_url.contains("://api.gab.ai") {
        return Some(base_url.replace("://api.gab.ai", "://gab.ai"));
    }
    if base_url.contains("://gab.ai") {
        return Some(base_url.replace("://gab.ai", "://api.gab.ai"));
    }
    None
}
