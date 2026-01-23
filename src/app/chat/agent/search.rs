use crate::app::chat::agent::intent::QueryIntent;

pub struct SearchContext {
    brave_key: String,
}

pub struct SearchSnapshotRequest<'a> {
    pub query: &'a str,
    pub intent: QueryIntent,
}

#[derive(Debug, Clone)]
enum SearchAction {
    BraveSearch { query: String },
}

impl SearchContext {
    pub fn new(brave_key: String) -> Self {
        Self { brave_key }
    }
}

pub fn enrich_prompt_with_search_snapshot(
    context: &SearchContext,
    prompt_lines: &mut Vec<String>,
    request: SearchSnapshotRequest<'_>,
) -> Option<String> {
    let action = select_search_action(request)?;
    match action {
        SearchAction::BraveSearch { query } => {
            append_brave_search_results_snapshot(context, prompt_lines, &query)
        }
    }
}

fn append_brave_search_results_snapshot(
    context: &SearchContext,
    prompt_lines: &mut Vec<String>,
    query: &str,
) -> Option<String> {
    if context.brave_key.trim().is_empty() {
        return Some(
            "Live search is not configured. Add a Brave API key in config.local.toml."
                .to_string(),
        );
    }
    match crate::agents::brave::search(&context.brave_key, query) {
        Ok(results) => {
            if results.is_empty() {
                return Some("I couldn't find any live search results for that.".to_string());
            }
            prompt_lines.push(
                "All temperatures must be in Celsius (metric units). Do not use Fahrenheit."
                    .to_string(),
            );
            prompt_lines.push(
                "Use only the search results below to answer. If they are missing or unclear, say you cannot find the up-to-date information."
                    .to_string(),
            );
            prompt_lines.push(
                "Use the Brave search results below to answer the user's request.".to_string(),
            );
            prompt_lines.push(format!(
                "Brave search results for \"{}\":\n{}",
                query, results
            ));
            None
        }
        Err(error) => Some(format!("Live search failed: {}", error)),
    }
}


pub(crate) fn should_use_brave_search(query: &str) -> bool {
    let trimmed = query.trim();
    let lowered = trimmed.to_lowercase();
    if lowered.is_empty() {
        return false;
    }
    if looks_like_weather_question(&lowered) {
        return false;
    }
    if looks_like_entity_query(trimmed) {
        return true;
    }
    let search_terms = [
        "search",
        "look up",
        "lookup",
        "find",
        "latest",
        "current",
        "today",
        "now",
        "news",
        "update",
        "release date",
        "price",
        "event",
        "happening",
        "what is going on",
        "schedule",
        "score",
        "stock",
        "crypto",
    ];
    if search_terms.iter().any(|term| lowered.contains(term)) {
        return true;
    }

    let has_time_cue = ["2024", "2025", "this week", "this month"]
        .iter()
        .any(|term| lowered.contains(term));
    if has_time_cue {
        return true;
    }

    let looks_like_question = lowered.contains('?') || lowered.starts_with("what ");
    let has_location = ["in ", "near ", "at "].iter().any(|token| lowered.contains(token));
    looks_like_question && has_location
}

fn select_search_action(request: SearchSnapshotRequest<'_>) -> Option<SearchAction> {
    if request.intent.is_personal_recap
        || request.intent.is_week_note
        || request.intent.is_note_lookup
        || request.intent.is_note_creation
    {
        return None;
    }
    if request.intent.is_external_event || should_use_brave_search(request.query) {
        return Some(SearchAction::BraveSearch {
            query: request.query.to_string(),
        });
    }
    None
}

pub fn should_mark_searching_for_intent(query: &str, intent: QueryIntent) -> bool {
    let request = SearchSnapshotRequest { query, intent };
    select_search_action(request).is_some()
}

fn looks_like_entity_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count > 4 {
        return false;
    }
    let has_separator =
        trimmed.contains('-') || trimmed.contains('.') || trimmed.contains('/') || trimmed.contains(':');
    let has_digit = trimmed.chars().any(|character| character.is_ascii_digit());
    let has_uppercase = trimmed.chars().any(|character| character.is_ascii_uppercase());
    has_separator || has_digit || (has_uppercase && word_count <= 3)
}

fn looks_like_weather_question(lowered: &str) -> bool {
    let weather_terms = [
        "weather",
        "forecast",
        "temperature",
        "temp",
        "rain",
        "snow",
        "wind",
        "humidity",
    ];
    weather_terms.iter().any(|term| {
        lowered.split_whitespace().any(|word| {
            let cleaned = word.trim_matches(|character: char| !character.is_alphanumeric());
            cleaned == *term
        })
    })
}

