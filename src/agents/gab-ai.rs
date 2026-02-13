use color_eyre::Result;

use crate::agents::openai_compat;

const DEFAULT_GAB_BASE_URL: &str = "https://gab.ai/v1";

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
    let request = openai_compat::OpenAIChatRequest {
        model,
        messages: openai_compat::convert_messages(messages),
        stream: false,
        tools: None,
    };

    let client = openai_compat::build_client()?;
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
                    let payload: openai_compat::OpenAIChatResponse = response.json()?;
                    return openai_compat::extract_reply(payload, "Gab AI");
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
