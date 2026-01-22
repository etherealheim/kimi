use crate::app::chat::agent::context::{is_personal_recap_query, is_week_note_query};
use crate::app::chat::agent::obsidian::is_external_event_query;
use crate::agents::{Agent, AgentManager};
use serde::Deserialize;

const SEARCH_DECISION_SYSTEM_PROMPT: &str = r#"You are a search routing assistant.
Decide whether a user's request needs live web search or can be answered directly.

Return ONLY valid JSON in this exact schema:
{"action":"search|direct|clarify","query":"<search query if action=search>"}

Rules:
- Use "search" for proper nouns, company names, products, recent info, or if uncertain.
- Use "clarify" if the request is ambiguous and needs a question first.
- Use "direct" if it is general knowledge and timeless.
- When action is "search", craft a concise query (2-6 words).
"#;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SearchDecisionAction {
    Search,
    Direct,
    Clarify,
}

#[derive(Debug, Clone)]
struct SearchDecision {
    action: SearchDecisionAction,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchDecisionPayload {
    action: String,
    query: Option<String>,
}

pub struct SearchContext {
    agent: Agent,
    manager: AgentManager,
    brave_key: String,
}

impl SearchContext {
    pub fn new(agent: Agent, manager: AgentManager, brave_key: String) -> Self {
        Self {
            agent,
            manager,
            brave_key,
        }
    }
}

pub fn enrich_prompt_with_search_snapshot(
    context: &SearchContext,
    prompt_lines: &mut Vec<String>,
    query: &str,
) -> Option<String> {
    let lowered = query.to_lowercase();
    // Week note queries should never trigger web search
    if is_personal_recap_query(&lowered) || is_week_note_query(&lowered) {
        return None;
    }
    // Explicit note queries should rely on Obsidian, not web search
    if is_note_lookup_query(&lowered) {
        return None;
    }
    if is_external_event_query(&lowered) {
        return append_brave_search_results_snapshot(context, prompt_lines, query);
    }
    let decision = decide_search_decision_snapshot(context, query);
    let action = decision.as_ref().map(|value| value.action);
    if action == Some(SearchDecisionAction::Clarify) {
        prompt_lines.push("Ask a brief clarifying question before answering.".to_string());
        return None;
    }
    let should_search = match action {
        Some(SearchDecisionAction::Search) => true,
        Some(SearchDecisionAction::Direct) | Some(SearchDecisionAction::Clarify) => false,
        None => should_use_brave_search(query),
    };
    if !should_search {
        return None;
    }
    let search_query = select_search_query(decision.as_ref(), query);
    append_brave_search_results_snapshot(context, prompt_lines, &search_query)
}

fn decide_search_decision_snapshot(
    context: &SearchContext,
    query: &str,
) -> Option<SearchDecision> {
    let messages = build_search_decision_messages(query);
    let response = context.manager.chat(&context.agent, &messages).ok()?;
    parse_search_decision(&response)
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
    match crate::services::brave::search(&context.brave_key, query) {
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

fn build_search_decision_messages(query: &str) -> Vec<crate::agents::ChatMessage> {
    vec![
        crate::agents::ChatMessage::system(SEARCH_DECISION_SYSTEM_PROMPT),
        crate::agents::ChatMessage::user(query),
    ]
}

fn parse_search_decision(response: &str) -> Option<SearchDecision> {
    let json = extract_json_object(response)?;
    let payload: SearchDecisionPayload = serde_json::from_str(&json).ok()?;
    let action = parse_search_decision_action(payload.action.trim())?;
    let query = payload.query.map(|value| value.trim().to_string());
    Some(SearchDecision { action, query })
}

fn parse_search_decision_action(value: &str) -> Option<SearchDecisionAction> {
    match value {
        "search" => Some(SearchDecisionAction::Search),
        "direct" => Some(SearchDecisionAction::Direct),
        "clarify" => Some(SearchDecisionAction::Clarify),
        _ => None,
    }
}

fn extract_json_object(value: &str) -> Option<String> {
    let start = value.find('{')?;
    let end = value.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(value[start..=end].to_string())
}

fn select_search_query(decision: Option<&SearchDecision>, fallback: &str) -> String {
    decision
        .and_then(|value| value.query.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| fallback.to_string())
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
    weather_terms.iter().any(|term| lowered.contains(term))
}

fn is_note_lookup_query(lowered: &str) -> bool {
    let note_indicators = [
        "in my notes",
        "from my notes",
        "in notes",
        "my notes about",
        "notes about",
        "wrote in my notes",
        "have in my notes",
        "in my obsidian",
        "from obsidian",
    ];
    note_indicators.iter().any(|term| lowered.contains(term))
}
