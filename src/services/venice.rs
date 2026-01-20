use color_eyre::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::thread::sleep;
use std::time::Duration;

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

#[derive(Debug, Deserialize)]
struct VeniceChatResponse {
    choices: Vec<VeniceChoice>,
}

#[derive(Debug, Deserialize)]
struct VeniceChoice {
    message: VeniceMessage,
}

#[derive(Debug, Deserialize)]
struct VeniceMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct VeniceChatRequest {
    model: String,
    messages: Vec<VeniceChatMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct VeniceChatMessage {
    role: String,
    content: String,
}

pub fn chat(api_key: &str, model: &str, messages: &[crate::agents::ChatMessage]) -> Result<String> {
    let chat_messages = messages
        .iter()
        .map(|msg| VeniceChatMessage {
            role: match msg.role {
                crate::agents::MessageRole::System => "system".to_string(),
                crate::agents::MessageRole::User => "user".to_string(),
                crate::agents::MessageRole::Assistant => "assistant".to_string(),
            },
            content: msg.content.clone(),
        })
        .collect();

    let request = VeniceChatRequest {
        model: model.to_string(),
        messages: chat_messages,
        stream: false,
    };

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()?;
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
                    let payload: VeniceChatResponse = response.json()?;
                    return payload
                        .choices
                        .get(0)
                        .map(|choice| choice.message.content.clone())
                        .ok_or_else(|| color_eyre::eyre::eyre!("Venice response missing content"));
                }

                if status.as_u16() == 429 || status.as_u16() >= 500 {
                    last_error = Some(color_eyre::eyre::eyre!(
                        "Venice API error ({}), retrying...",
                        status
                    ));
                } else {
                    return Err(color_eyre::eyre::eyre!(
                        "Venice API error: {}",
                        status
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
