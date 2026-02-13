use color_eyre::Result;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const DEFAULT_RESULT_COUNT: u8 = 5;
const REQUEST_CONNECT_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 10;

// --- Response structs ---

#[derive(Debug, Deserialize)]
struct BraveWebResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveSearchResult>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Fields deserialized from API response for future use
pub struct BraveSearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
    #[serde(default)]
    pub extra_snippets: Vec<String>,
    #[serde(default)]
    pub page_age: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub profile: Option<BraveResultProfile>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Deserialized from API response
pub struct BraveResultProfile {
    #[serde(default)]
    pub long_name: Option<String>,
}

// --- Search parameters ---

/// Parameters for Brave Web Search API requests
pub struct BraveSearchParams {
    /// Number of results to return (1-20)
    pub count: u8,
    /// Request extra snippets for richer context (up to 5 per result)
    pub extra_snippets: bool,
    /// Freshness filter: "pd" (day), "pw" (week), "pm" (month), "py" (year), or date range
    pub freshness: Option<String>,
    /// Disable text decorations (HTML bold tags) for cleaner LLM input
    pub text_decorations: bool,
}

impl Default for BraveSearchParams {
    fn default() -> Self {
        Self {
            count: DEFAULT_RESULT_COUNT,
            extra_snippets: true,
            freshness: None,
            text_decorations: false,
        }
    }
}

// --- Search function ---

/// Performs a Brave Web Search and returns structured results
pub fn search(api_key: &str, query: &str, params: &BraveSearchParams) -> Result<Vec<BraveSearchResult>> {
    if api_key.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!("Brave API key not configured"));
    }
    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(REQUEST_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()?;

    let mut query_pairs: Vec<(&str, String)> = vec![
        ("q", trimmed_query.to_string()),
        ("count", params.count.to_string()),
        ("extra_snippets", params.extra_snippets.to_string()),
        ("text_decorations", params.text_decorations.to_string()),
    ];

    if let Some(freshness) = &params.freshness {
        query_pairs.push(("freshness", freshness.clone()));
    }

    let response = client
        .get(BRAVE_SEARCH_URL)
        .header("X-Subscription-Token", api_key)
        .query(&query_pairs)
        .send()?
        .error_for_status()?;

    let payload: BraveWebResponse = response.json()?;
    let results = payload
        .web
        .map(|web| web.results)
        .unwrap_or_default();

    Ok(results)
}

// --- LLM-optimized formatting ---

/// Formats search results into structured blocks optimized for LLM consumption.
///
/// Each result includes source domain, publication date, main description,
/// and up to 5 additional context snippets.
pub fn format_results_for_llm(results: &[BraveSearchResult]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut blocks = Vec::new();
    for (index, result) in results.iter().enumerate() {
        let mut block = format!("[{}] {}", index + 1, result.title.trim());

        // Source and date metadata line
        let domain = result
            .profile
            .as_ref()
            .and_then(|profile| profile.long_name.clone())
            .unwrap_or_else(|| extract_domain(&result.url));
        let metadata = build_metadata_line(&domain, &result.page_age);
        if !metadata.is_empty() {
            block.push_str(&format!("\n{}", metadata));
        }

        // Main description
        block.push_str(&format!("\n{}", result.description.trim()));

        // Extra snippets for additional context
        if !result.extra_snippets.is_empty() {
            block.push_str("\nAdditional context:");
            for snippet in &result.extra_snippets {
                let trimmed = snippet.trim();
                if !trimmed.is_empty() {
                    block.push_str(&format!("\n- {}", trimmed));
                }
            }
        }

        blocks.push(block);
    }

    blocks.join("\n\n")
}

/// Extracts the domain name from a URL for source attribution
fn extract_domain(url: &str) -> String {
    url.split("//")
        .nth(1)
        .and_then(|after_scheme| after_scheme.split('/').next())
        .unwrap_or(url)
        .trim_start_matches("www.")
        .to_string()
}

/// Builds the metadata line with source and optional publication date
fn build_metadata_line(domain: &str, page_age: &Option<String>) -> String {
    let mut parts = Vec::new();
    if !domain.is_empty() {
        parts.push(format!("Source: {}", domain));
    }
    if let Some(age) = page_age {
        let date_display = format_page_age(age);
        if !date_display.is_empty() {
            parts.push(format!("Published: {}", date_display));
        }
    }
    parts.join(" | ")
}

/// Formats the page_age ISO timestamp into a readable date (YYYY-MM-DD)
fn format_page_age(raw: &str) -> String {
    // page_age comes as ISO 8601 like "2026-02-12T17:41:12"
    // Extract just the date portion
    raw.split('T')
        .next()
        .unwrap_or(raw)
        .to_string()
}
