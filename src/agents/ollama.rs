use crate::agents::{ChatMessage, MessageRole};
use color_eyre::Result;
use reqwest::blocking::Client;
use std::time::Duration;
use serde::{Deserialize, Serialize};

pub struct OllamaClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    images: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    pub fn chat(&self, model: &str, messages: &[ChatMessage]) -> Result<String> {
        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|msg| OllamaMessage {
                role: match msg.role {
                    MessageRole::System => "system".to_string(),
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                },
                content: msg.content.clone(),
                images: msg.images.clone(),
            })
            .collect();

        let request = OllamaChatRequest {
            model: model.to_string(),
            messages: ollama_messages,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()?;

        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(color_eyre::eyre::eyre!(
                "Ollama chat failed ({}): {}",
                status,
                body
            ));
        }

        let chat_response: OllamaChatResponse = serde_json::from_str(&body)?;
        Ok(chat_response.message.content)
    }

    pub fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .is_ok()
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct ModelList {
            models: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(2))
            .send()?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let model_list: ModelList = response.json()?;
        Ok(model_list.models.into_iter().map(|model| model.name).collect())
    }

    pub fn check_model(&self, model: &str) -> Result<bool> {
        #[derive(Deserialize)]
        struct ModelList {
            models: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let model_list: ModelList = response.json()?;
        Ok(model_list
            .models
            .iter()
            .any(|m| model_name_matches(&m.name, model)))
    }
}

fn model_name_matches(available: &str, requested: &str) -> bool {
    if available == requested || available.starts_with(&format!("{requested}:")) {
        return true;
    }

    let requested_base = requested.split(':').next().unwrap_or(requested);
    let available_base = available.split(':').next().unwrap_or(available);
    if available_base == requested_base {
        return true;
    }

    let requested_last = requested_base.rsplit('/').next().unwrap_or(requested_base);
    let available_last = available_base.rsplit('/').next().unwrap_or(available_base);
    requested_last == available_last
}
