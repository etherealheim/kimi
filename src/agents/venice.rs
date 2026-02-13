use color_eyre::Result;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::thread::sleep;
use std::time::Duration;

use crate::agents::openai_compat::{self, ChatResponse, ToolDefinition};

const VENICE_MODELS_URL: &str = "https://api.venice.ai/api/v1/models?type=text";
const VENICE_CHAT_URL: &str = "https://api.venice.ai/api/v1/chat/completions";

#[derive(Debug, Deserialize)]
struct VeniceModelsResponse {
    data: Vec<VeniceModel>,
}

#[derive(Debug, Deserialize)]
struct VeniceModel {
    id: String,
}

pub fn fetch_text_models(api_key: &str) -> Result<Vec<String>> {
    let client = Client::new();
    let response = client
        .get(VENICE_MODELS_URL)
        .bearer_auth(api_key)
        .timeout(Duration::from_secs(2))
        .send()?
        .error_for_status()?;

    let payload: VeniceModelsResponse = response.json()?;
    Ok(payload.data.into_iter().map(|model| model.id).collect())
}

pub fn chat(api_key: &str, model: &str, messages: &[crate::agents::ChatMessage]) -> Result<String> {
    let request = openai_compat::OpenAIChatRequest {
        model: model.to_string(),
        messages: openai_compat::convert_messages(messages),
        stream: false,
        tools: None,
    };

    let client = openai_compat::build_client()?;
    let mut last_error: Option<color_eyre::Report> = None;
    let delays = [200, 500, 1000];
    for (attempt, delay) in delays.iter().enumerate() {
        let response = client
            .post(VENICE_CHAT_URL)
            .bearer_auth(api_key)
            .json(&request)
            .send();

        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let payload: openai_compat::OpenAIChatResponse = response.json()?;
                    return openai_compat::extract_reply(payload, "Venice");
                }

                let details = response.text().unwrap_or_default();
                if status.as_u16() == 429 || status.as_u16() >= 500 {
                    last_error = Some(color_eyre::eyre::eyre!(
                        "Venice API error ({}), retrying... {}",
                        status,
                        details
                    ));
                } else {
                    return Err(color_eyre::eyre::eyre!(
                        "Venice API error: {} {}",
                        status,
                        details
                    ));
                }
            }
            Err(error) => {
                last_error = Some(color_eyre::eyre::eyre!(
                    "Venice request error: {}",
                    error
                ));
            }
        }

        if attempt < delays.len() - 1 {
            sleep(Duration::from_millis(*delay));
        }
    }

    Err(last_error.unwrap_or_else(|| {
        color_eyre::eyre::eyre!("Venice request failed after retries")
    }))
}

/// Sends a chat request with native tool calling support
/// Returns a ChatResponse with both content and any tool calls the model wants to make
pub fn chat_with_tools(
    api_key: &str,
    model: &str,
    messages: &[crate::agents::ChatMessage],
    tools: &[ToolDefinition],
) -> Result<ChatResponse> {
    let tools_payload = if tools.is_empty() {
        None
    } else {
        Some(tools.to_vec())
    };

    let request = openai_compat::OpenAIChatRequest {
        model: model.to_string(),
        messages: openai_compat::convert_messages(messages),
        stream: false,
        tools: tools_payload,
    };

    let client = openai_compat::build_client()?;
    let mut last_error: Option<color_eyre::Report> = None;
    let delays = [200, 500, 1000];
    for (attempt, delay) in delays.iter().enumerate() {
        let response = client
            .post(VENICE_CHAT_URL)
            .bearer_auth(api_key)
            .json(&request)
            .send();

        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let payload: openai_compat::OpenAIChatResponse = response.json()?;
                    return openai_compat::extract_chat_response(payload, "Venice");
                }

                let details = response.text().unwrap_or_default();
                if status.as_u16() == 429 || status.as_u16() >= 500 {
                    last_error = Some(color_eyre::eyre::eyre!(
                        "Venice API error ({}), retrying... {}",
                        status,
                        details
                    ));
                } else {
                    return Err(color_eyre::eyre::eyre!(
                        "Venice API error: {} {}",
                        status,
                        details
                    ));
                }
            }
            Err(error) => {
                last_error = Some(color_eyre::eyre::eyre!(
                    "Venice request error: {}",
                    error
                ));
            }
        }

        if attempt < delays.len() - 1 {
            sleep(Duration::from_millis(*delay));
        }
    }

    Err(last_error.unwrap_or_else(|| {
        color_eyre::eyre::eyre!("Venice request failed after retries")
    }))
}
