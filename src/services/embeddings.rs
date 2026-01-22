use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::config::Config;

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Generates embeddings using the configured Ollama model
pub async fn generate_embedding(text: &str) -> Result<Vec<f32>> {
    let config = Config::load()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let response = client
        .post(format!("{}/api/embed", config.embeddings.ollama_url))
        .json(&EmbedRequest {
            model: config.embeddings.model,
            input: text.to_string(),
        })
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(color_eyre::eyre::eyre!(
            "Ollama embed failed ({}): {}",
            status,
            body
        ));
    }
    let response: EmbedResponse = serde_json::from_str(&body)?;
    
    response
        .embeddings
        .into_iter()
        .next()
        .ok_or_else(|| color_eyre::eyre::eyre!("No embedding returned"))
}
