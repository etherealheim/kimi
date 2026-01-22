use color_eyre::Result;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";

#[derive(Debug, Deserialize)]
struct BraveWebResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveSearchResult>,
}

#[derive(Debug, Deserialize)]
struct BraveSearchResult {
    title: String,
    url: String,
    description: String,
}

pub fn search(api_key: &str, query: &str) -> Result<String> {
    if api_key.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!("Brave API key not configured"));
    }
    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return Ok(String::new());
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get(BRAVE_SEARCH_URL)
        .header("X-Subscription-Token", api_key)
        .query(&[("q", trimmed_query), ("source", "web")])
        .send()?
        .error_for_status()?;

    let payload: BraveWebResponse = response.json()?;
    let results = payload
        .web
        .map(|web| web.results)
        .unwrap_or_default();

    if results.is_empty() {
        return Ok(String::new());
    }

    let mut lines = Vec::new();
    for (index, result) in results.into_iter().take(10).enumerate() {
        lines.push(format!(
            "{}. {} ({})\n   {}",
            index + 1,
            result.title.trim(),
            result.url.trim(),
            result.description.trim()
        ));
    }

    Ok(lines.join("\n"))
}
