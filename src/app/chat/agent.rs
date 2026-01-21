use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, TextInput};
use crate::app::AgentEvent;
use crate::storage::ConversationSummary;
use chrono::{Datelike, Duration, Local};
use color_eyre::Result;
use serde::Deserialize;

impl App {
    fn model_source_for(
        &self,
        agent_name: &str,
        model_name: &str,
    ) -> Option<crate::app::ModelSource> {
        self.available_models
            .get(agent_name)
            .and_then(|models| models.iter().find(|model| model.name == model_name))
            .map(|model| model.source.clone())
    }

    pub fn is_agent_command(&self, command: &str) -> bool {
        matches!(command, "translate" | "chat")
    }

    /// Rotates between chat and translate agents
    pub fn rotate_agent(&mut self) -> Result<()> {
        let current_agent_name = self.current_agent.as_ref().map(|agent| agent.name.as_str());

        let next_agent = match current_agent_name {
            Some("chat") => "translate",
            Some("translate") => "chat",
            _ => "chat", // Default to chat if no agent or unknown agent
        };

        self.load_agent(next_agent)
    }

    pub fn load_agent(&mut self, agent_name: &str) -> Result<()> {
        self.reset_chat_scroll();

        if let Some(current_agent) = &self.current_agent {
            self.chat_history_by_agent
                .insert(current_agent.name.clone(), self.chat_history.clone());
            self.personality_enabled_by_agent
                .insert(current_agent.name.clone(), self.personality_enabled);
        }

        if agent_name == "translate" {
            self.personality_enabled = false;
            self.personality_text = None;
        } else if let Some(is_enabled) = self.personality_enabled_by_agent.get(agent_name).copied()
        {
            self.personality_enabled = is_enabled;
            if !self.personality_enabled {
                self.personality_text = None;
            }
        }

        let selected_model = self
            .selected_models
            .get(agent_name)
            .and_then(|models| models.first())
            .cloned();
        let selected_source = selected_model
            .as_ref()
            .and_then(|model_name| self.model_source_for(agent_name, model_name))
            .unwrap_or(crate::app::ModelSource::Ollama);

        let manager = self
            .agent_manager
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Agent manager not initialized"))?;
        let mut agent = manager
            .get_agent(agent_name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Agent '{}' not found", agent_name))?
            .clone();

        if let Some(model_name) = selected_model {
            agent.model = model_name;
        }
        agent.model_source = selected_source;

        self.current_agent = Some(agent.clone());
        self.chat_history = self
            .chat_history_by_agent
            .get(agent_name)
            .cloned()
            .unwrap_or_default();
        self.chat_input = TextInput::new();
        self.chat_attachments.clear();
        self.mode = AppMode::Chat;

        if let Err(error) = manager.check_agent_ready(&agent) {
            self.chat_history.push(ChatMessage {
                role: MessageRole::System,
                content: format!("⚠️  {} agent not ready: {}", agent_name, error),
                timestamp: Local::now().format("%H:%M:%S").to_string(),
                display_name: None,
            });
        }
        Ok(())
    }

    /// Extracts agent chat dependencies (agent, manager, channel)
    pub(crate) fn get_agent_chat_dependencies(
        &self,
    ) -> Result<(
        crate::agents::Agent,
        crate::agents::AgentManager,
        std::sync::mpsc::Sender<AgentEvent>,
    )> {
        let agent = self
            .current_agent
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("No agent selected"))?
            .clone();
        let manager = self
            .agent_manager
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Agent manager not initialized"))?
            .clone();
        let agent_tx = self
            .agent_tx
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Agent channel not initialized"))?
            .clone();

        Ok((agent, manager, agent_tx))
    }

    /// Converts chat history to agent messages format
    pub(crate) fn build_agent_messages(&mut self, system_prompt: &str) -> Vec<AgentChatMessage> {
        if self.personality_enabled && self.personality_text.is_none() {
            let selected_name = self
                .personality_name
                .clone()
                .unwrap_or_else(crate::services::personality::default_personality_name);
            if let Ok(text) = crate::services::personality::read_personality(&selected_name) {
                if !text.trim().is_empty() {
                    self.personality_text = Some(text);
                }
            }
            self.personality_name = Some(selected_name);
        }

        let mut prompt_lines = vec![system_prompt.to_string()];
        let now = chrono::Local::now();
        prompt_lines.push(format!(
            "Current date and time: {}",
            now.format("%Y-%m-%d %H:%M:%S")
        ));
        prompt_lines.push("Respond in plain text. Do not use Markdown formatting.".to_string());
        if let Ok(profile_text) = crate::services::personality::read_my_personality() {
            let blocks = parse_user_context_blocks(&profile_text);
            let query = self
                .last_user_message_content()
                .unwrap_or_default()
                .to_lowercase();
            for block in blocks {
                match block.kind {
                    UserContextKind::Always => {
                        if !block.content.is_empty() {
                            prompt_lines.push(format!(
                                "User context (always):\n{}",
                                block.content
                            ));
                        }
                    }
                    UserContextKind::Context { tag } => {
                        if !block.content.is_empty()
                            && should_include_user_context(&query, &tag, &block.content)
                        {
                            prompt_lines.push(format!(
                                "User context ({}):\n{}",
                                tag, block.content
                            ));
                        }
                    }
                }
            }
        }
        if let Some(query) = self.last_user_message_content() {
            if let Ok(Some(summary_context)) =
                build_conversation_summary_context(self.storage.as_ref(), &query)
            {
                if !summary_context.is_empty() {
                    prompt_lines.push("--- Conversation summaries ---".to_string());
                    prompt_lines.push(summary_context);
                    prompt_lines.push(
                        "Use the summaries above to answer recap questions. If they are insufficient, ask a clarifying question."
                            .to_string(),
                    );
                }
            }
        }
        if let Ok(memories) = crate::services::memories::read_memories() {
            let trimmed = memories.trim();
            if !trimmed.is_empty() {
                prompt_lines.push("--- Memories ---".to_string());
                prompt_lines.push(trimmed.to_string());
                prompt_lines.push(
                    "Use the memories above as persistent user facts. Prefer matching context tags and ignore low-confidence items unless confirmed."
                        .to_string(),
                );
            }
        }
        if let Some(query) = self.last_user_message_content() {
            self.enrich_prompt_with_search(&mut prompt_lines, &query);
        }
        if self.personality_enabled {
            if let Some(text) = &self.personality_text {
                if !text.trim().is_empty() {
                    prompt_lines.push(text.trim().to_string());
                }
            }
        }
        let merged_prompt = prompt_lines.join("\n\n");

        let mut messages = vec![AgentChatMessage::system(merged_prompt)];
        for chat_message in &self.chat_history {
            if chat_message.role == MessageRole::User {
                messages.push(AgentChatMessage::user(&chat_message.content));
            } else if chat_message.role == MessageRole::Assistant {
                messages.push(AgentChatMessage::assistant(&chat_message.content));
            }
        }
        messages
    }

    /// Spawns a background thread to process the agent chat request
    pub(crate) fn spawn_agent_chat_thread(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        messages: Vec<AgentChatMessage>,
        agent_tx: std::sync::mpsc::Sender<AgentEvent>,
    ) {
        std::thread::spawn(move || {
            let _ = match manager.chat(&agent, &messages) {
                Ok(response) => agent_tx.send(AgentEvent::Response(response)),
                Err(error) => agent_tx.send(AgentEvent::Error(error.to_string())),
            };
        });
    }

    pub(crate) fn last_user_message_content(&self) -> Option<String> {
        self.chat_history
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.clone())
    }

    fn enrich_prompt_with_search(&mut self, prompt_lines: &mut Vec<String>, query: &str) {
        let decision = self.decide_search_decision(query);
        let action = decision.as_ref().map(|value| value.action);
        if action == Some(SearchDecisionAction::Clarify) {
            prompt_lines.push("Ask a brief clarifying question before answering.".to_string());
            return;
        }
        let should_search = match action {
            Some(SearchDecisionAction::Search) => true,
            Some(SearchDecisionAction::Direct) | Some(SearchDecisionAction::Clarify) => false,
            None => should_use_brave_search(query),
        };
        if !should_search {
            return;
        }
        let search_query = select_search_query(decision.as_ref(), query);
        self.append_brave_search_results(prompt_lines, &search_query);
    }

    fn decide_search_decision(&self, query: &str) -> Option<SearchDecision> {
        let agent = self.current_agent.as_ref()?;
        let manager = self.agent_manager.as_ref()?;
        let messages = build_search_decision_messages(query);
        let response = manager.chat(agent, &messages).ok()?;
        parse_search_decision(&response)
    }

    fn append_brave_search_results(&mut self, prompt_lines: &mut Vec<String>, query: &str) {
        if self.connect_brave_key.trim().is_empty() {
            self.pending_search_notice = Some(
                "Live search is not configured. Add a Brave API key in config.local.toml."
                    .to_string(),
            );
            return;
        }
        match crate::services::brave::search(&self.connect_brave_key, query) {
            Ok(results) => {
                if results.is_empty() {
                    self.pending_search_notice =
                        Some("I couldn't find any live search results for that.".to_string());
                    return;
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
                    "Use the Brave search results below to answer the user's request."
                        .to_string(),
                );
                prompt_lines.push(format!(
                    "Brave search results for \"{}\":\n{}",
                    query, results
                ));
            }
            Err(error) => {
                self.pending_search_notice = Some(format!("Live search failed: {}", error));
            }
        }
    }
}

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

fn build_search_decision_messages(query: &str) -> Vec<AgentChatMessage> {
    vec![
        AgentChatMessage::system(SEARCH_DECISION_SYSTEM_PROMPT),
        AgentChatMessage::user(query),
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

fn build_conversation_summary_context(
    storage: Option<&crate::storage::StorageManager>,
    query: &str,
) -> Result<Option<String>> {
    let Some(storage) = storage else {
        return Ok(None);
    };
    let Some(range) = summary_time_range(query) else {
        return Ok(None);
    };
    let conversations = storage.load_conversations()?;
    let summaries = filter_summaries_by_range(&conversations, range);
    if summaries.is_empty() {
        return Ok(None);
    }
    let mut lines = Vec::new();
    for entry in summaries {
        lines.push(format!("- {}: {}", entry.date, entry.summary));
    }
    Ok(Some(lines.join("\n")))
}

#[derive(Debug, Clone)]
struct SummaryEntry {
    date: String,
    summary: String,
}

#[derive(Debug, Clone, Copy)]
enum SummaryRange {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    LastDays(u32),
}

fn summary_time_range(query: &str) -> Option<SummaryRange> {
    let lowered = query.to_lowercase();
    if !has_summary_intent(&lowered) {
        return None;
    }
    if lowered.contains("yesterday") {
        return Some(SummaryRange::Yesterday);
    }
    if lowered.contains("today") {
        return Some(SummaryRange::Today);
    }
    if lowered.contains("last week") {
        return Some(SummaryRange::LastWeek);
    }
    if lowered.contains("this week") {
        return Some(SummaryRange::ThisWeek);
    }
    if let Some(days) = parse_last_days(&lowered) {
        return Some(SummaryRange::LastDays(days));
    }
    None
}

fn has_summary_intent(lowered: &str) -> bool {
    let triggers = [
        "remember",
        "recap",
        "summary",
        "what happened",
        "what did we",
        "what have we",
        "catch me up",
        "what were we",
        "what we talked",
    ];
    triggers.iter().any(|term| lowered.contains(term))
}

fn parse_last_days(lowered: &str) -> Option<u32> {
    let tokens: Vec<&str> = lowered.split_whitespace().collect();
    for window in tokens.windows(3) {
        if let [number, "days", "ago"] = window {
            if let Ok(value) = number.parse::<u32>() {
                return Some(value);
            }
        }
    }
    for window in tokens.windows(3) {
        if let [number, "days", "back"] = window {
            if let Ok(value) = number.parse::<u32>() {
                return Some(value);
            }
        }
    }
    None
}

fn filter_summaries_by_range(
    conversations: &[ConversationSummary],
    range: SummaryRange,
) -> Vec<SummaryEntry> {
    let today = Local::now().date_naive();
    let (start, end) = match range {
        SummaryRange::Today => (today, today),
        SummaryRange::Yesterday => (today - Duration::days(1), today - Duration::days(1)),
        SummaryRange::ThisWeek => {
            let days_from_monday = today.weekday().num_days_from_monday() as i64;
            (today - Duration::days(days_from_monday), today)
        }
        SummaryRange::LastWeek => (today - Duration::days(7), today - Duration::days(1)),
        SummaryRange::LastDays(days) => {
            let span = i64::from(days.max(1));
            (today - Duration::days(span), today)
        }
    };
    let mut entries = Vec::new();
    for convo in conversations {
        let Some(date) = parse_conversation_date(&convo.created_at) else {
            continue;
        };
        if date < start || date > end {
            continue;
        }
        let summary = convo
            .summary
            .clone()
            .or_else(|| convo.detailed_summary.clone())
            .unwrap_or_else(|| "Conversation".to_string());
        entries.push(SummaryEntry {
            date: date.format("%Y-%m-%d").to_string(),
            summary,
        });
    }
    entries
}

fn parse_conversation_date(created_at: &str) -> Option<chrono::NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map(|value| value.date_naive())
}

#[derive(Debug, Clone)]
enum UserContextKind {
    Always,
    Context { tag: String },
}

#[derive(Debug, Clone)]
struct UserContextBlock {
    kind: UserContextKind,
    content: String,
}

fn parse_user_context_blocks(profile_text: &str) -> Vec<UserContextBlock> {
    let mut blocks = Vec::new();
    let mut current_kind: Option<UserContextKind> = None;
    let mut current_lines: Vec<String> = Vec::new();

    let flush_block = |blocks: &mut Vec<UserContextBlock>,
                       kind: &mut Option<UserContextKind>,
                       lines: &mut Vec<String>| {
        if let Some(kind) = kind.take() {
            let content = lines.join("\n").trim().to_string();
            blocks.push(UserContextBlock { kind, content });
        }
        lines.clear();
    };

    for line in profile_text.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("[always]") {
            flush_block(&mut blocks, &mut current_kind, &mut current_lines);
            current_kind = Some(UserContextKind::Always);
            continue;
        }
        if let Some(tag) = trimmed
            .strip_prefix("[context:")
            .and_then(|value| value.strip_suffix(']'))
        {
            flush_block(&mut blocks, &mut current_kind, &mut current_lines);
            current_kind = Some(UserContextKind::Context {
                tag: tag.trim().to_lowercase(),
            });
            continue;
        }
        current_lines.push(line.to_string());
    }

    flush_block(&mut blocks, &mut current_kind, &mut current_lines);
    blocks
}

fn should_include_user_context(query: &str, tag: &str, content: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    if query.contains(tag) {
        return true;
    }

    let keywords = extract_context_keywords(content);
    keywords.iter().any(|keyword| query.contains(keyword))
}

fn extract_context_keywords(content: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    for token in content.split(|character: char| !character.is_alphanumeric()) {
        let lowered = token.trim().to_lowercase();
        if lowered.len() < 3 {
            continue;
        }
        keywords.push(lowered);
    }
    keywords.sort();
    keywords.dedup();
    keywords
}
