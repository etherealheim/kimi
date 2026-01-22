mod context;
pub(crate) mod intent;
mod json;
pub(crate) mod obsidian;
pub(crate) mod search;
mod verification;


use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, ContextUsage, TextInput};
use crate::app::AgentEvent;
use crate::app::chat::agent::context::{
    build_conversation_summary_entries,
    count_summary_matches,
    format_summary_entries,
    build_memory_context,
    tokenize_query,
};
use crate::app::chat::agent::intent::{classify_query_with_model, IntentModelContext, QueryIntent};
use chrono::Local;
use color_eyre::Result;

impl App {
    fn is_gab_model_name(&self, model_name: &str) -> bool {
        model_name.trim().eq_ignore_ascii_case("arya")
    }

    fn model_source_for(
        &self,
        agent_name: &str,
        model_name: &str,
    ) -> Option<crate::app::ModelSource> {
        let source = self
            .available_models
            .get(agent_name)
            .and_then(|models| {
                models
                    .iter()
                    .find(|model| model_name_matches_case_insensitive(&model.name, model_name))
            })
            .map(|model| model.source.clone());
        if source.is_some() {
            return source;
        }
        if agent_name == "chat"
            && self.is_gab_model_name(model_name)
            && !self.connect_gab_key.trim().is_empty()
        {
            return Some(crate::app::ModelSource::GabAI);
        }
        None
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
            if self.personality_enabled {
                let selected_name = self
                    .personality_name
                    .clone()
                    .unwrap_or_else(crate::services::personality::default_personality_name);
                if let Ok(text) = crate::services::personality::read_personality(&selected_name) {
                    if !text.trim().is_empty() {
                        self.personality_text = Some(text);
                    }
                }
            } else {
                self.personality_text = None;
            }
        }

        let selected_model = self
            .selected_models
            .get(agent_name)
            .and_then(|models| models.first())
            .cloned();
        let mut selected_source = selected_model
            .as_ref()
            .and_then(|model_name| self.model_source_for(agent_name, model_name))
            .unwrap_or(crate::app::ModelSource::Ollama);
        if agent_name == "chat"
            && selected_source == crate::app::ModelSource::Ollama
            && selected_model
                .as_ref()
                .map(|model_name| self.is_gab_model_name(model_name))
                .unwrap_or(false)
            && !self.connect_gab_key.trim().is_empty()
        {
            selected_source = crate::app::ModelSource::GabAI;
        }

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
                context_usage: None,
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


    pub(crate) fn spawn_agent_chat_thread_with_context(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        messages: Vec<AgentChatMessage>,
        system_context: String,
        should_verify: bool,
        agent_tx: std::sync::mpsc::Sender<AgentEvent>,
        context_usage: Option<ContextUsage>,
    ) {
        std::thread::spawn(move || {
            match manager.chat(&agent, &messages) {
                Ok(response) => {
                    if should_verify
                        && !response.trim().is_empty()
                        && verification::should_verify_response(&system_context)
                    {
                        let verify_messages =
                            verification::build_verification_messages(&system_context, &response);
                        if let Ok(verified) = manager.chat(&agent, &verify_messages) {
                            if !verified.trim().is_empty() {
                                let context_usage_for_verify = context_usage.clone();
                                let _ = agent_tx.send(AgentEvent::ResponseWithContext {
                                    response: verified,
                                    context_usage: context_usage_for_verify,
                                });
                                return;
                            }
                        }
                    }
                    let _ = agent_tx.send(AgentEvent::ResponseWithContext {
                        response,
                        context_usage,
                    });
                }
                Err(error) => {
                    let _ = agent_tx.send(AgentEvent::Error(error.to_string()));
                }
            }
        });
    }

    pub(crate) fn last_user_message_content(&self) -> Option<String> {
        self.chat_history
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.clone())
    }

}

fn enrich_query_with_context(query: &str, history: &[ChatMessage]) -> String {
    if !is_follow_up_query(query) {
        return query.to_string();
    }
    let previous_user_messages: Vec<&str> = history
        .iter()
        .rev()
        .filter(|msg| msg.role == MessageRole::User)
        .take(2)
        .map(|msg| msg.content.as_str())
        .collect();
    let Some(previous_query) = previous_user_messages.get(1) else {
        return query.to_string();
    };
    let previous_tokens = extract_meaningful_tokens(previous_query);
    if previous_tokens.is_empty() {
        return query.to_string();
    }
    format!("{} {}", query, previous_tokens.join(" "))
}

fn is_follow_up_query(query: &str) -> bool {
    let lowered = query.to_lowercase();
    let word_count = query.split_whitespace().count();
    if word_count > 12 {
        return false;
    }
    let pronouns = [" it", " that", " them", " those", " this", " these"];
    pronouns.iter().any(|pronoun| lowered.contains(pronoun))
}

fn query_wants_full_note_display(query: &str) -> bool {
    let lowered = query.to_lowercase();
    let full_display_terms = [
        "bring it",
        "bring that",
        "show it",
        "show that",
        "display it",
        "display that",
        "give it",
        "give that",
        "the whole note",
        "full note",
        "entire note",
        "everything",
        "all of it",
        "show me everything",
        "show me all",
        "give me everything",
        "paste it",
        "paste that",
        "copy it",
        "copy that",
    ];
    full_display_terms
        .iter()
        .any(|term| lowered.contains(term))
}

fn extract_meaningful_tokens(text: &str) -> Vec<String> {
    let stop_words = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
        "of", "with", "by", "from", "up", "about", "into", "through", "during",
        "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "can", "could", "will", "would", "should", "may",
        "might", "must", "i", "you", "he", "she", "it", "we", "they", "my",
        "your", "his", "her", "its", "our", "their", "what", "when", "where",
        "why", "how", "which", "who", "whom", "notes", "note", "obsidian",
    ];
    text.split_whitespace()
        .filter_map(|word| {
            let cleaned = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if cleaned.len() < 3 || stop_words.contains(&cleaned.as_str()) {
                None
            } else {
                Some(cleaned)
            }
        })
        .collect()
}

pub(crate) struct ChatBuildSnapshot {
    pub system_prompt: String,
    pub chat_history: Vec<ChatMessage>,
    pub personality_enabled: bool,
    pub personality_text: Option<String>,
    pub personality_name: Option<String>,
    pub connect_obsidian_vault: String,
    pub connect_brave_key: String,
}

pub(crate) struct ChatBuildResultWithUsage {
    pub messages: Vec<AgentChatMessage>,
    pub system_context: String,
    pub should_verify: bool,
    pub context_usage: Option<ContextUsage>,
    pub pending_search_notice: Option<String>,
}

pub(crate) fn build_agent_messages_from_snapshot(
    snapshot: ChatBuildSnapshot,
    agent: &crate::agents::Agent,
    manager: &crate::agents::AgentManager,
) -> ChatBuildResultWithUsage {
    let mut personality_text = snapshot.personality_text.clone();
    if snapshot.personality_enabled && personality_text.is_none() {
        let selected_name = snapshot
            .personality_name
            .clone()
            .unwrap_or_else(crate::services::personality::default_personality_name);
        if let Ok(text) = crate::services::personality::read_personality(&selected_name) {
            if !text.trim().is_empty() {
                personality_text = Some(text);
            }
        }
    }

    let last_user_message = snapshot
        .chat_history
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.content.clone());

    let mut prompt_lines = vec![snapshot.system_prompt.to_string()];
    let now = chrono::Local::now();
    prompt_lines.push(format!(
        "Current date and time: {}",
        now.format("%Y-%m-%d %H:%M:%S")
    ));
    prompt_lines.push("Respond in plain text. Do not use Markdown formatting.".to_string());
    prompt_lines.push("Respond in English unless the user asks otherwise.".to_string());
    if let Ok(profile_text) = crate::services::personality::read_my_personality() {
        let blocks = parse_user_context_blocks(&profile_text);
        let query = last_user_message.clone().unwrap_or_default().to_lowercase();
        for block in blocks {
            match block.kind {
                UserContextKind::Always => {
                    if !block.content.is_empty() {
                        prompt_lines.push(format!("User context (always):\n{}", block.content));
                    }
                }
                UserContextKind::Context { tag } => {
                    if !block.content.is_empty()
                        && should_include_user_context(&query, &tag, &block.content)
                    {
                        prompt_lines.push(format!("User context ({}):\n{}", tag, block.content));
                    }
                }
            }
        }
    }

    let mut context_usage = ContextUsage {
        notes_used: 0,
        history_used: 0,
        memories_used: 0,
    };
    let mut query_intent: Option<QueryIntent> = None;
    let storage = crate::storage::StorageManager::new().ok();
    let routing_agent = manager.get_agent("routing").cloned();
    if let Some(query) = last_user_message.clone() {
        let query_tokens = tokenize_query(&query);
        let intent_context = IntentModelContext {
            manager,
            routing_agent: routing_agent.as_ref(),
            fallback_agent: agent,
        };
        query_intent = Some(classify_query_with_model(&query, intent_context));
        if let Ok(summary_entries) = build_conversation_summary_entries(storage.as_ref(), &query) {
            if !summary_entries.is_empty() {
                context_usage.history_used =
                    count_summary_matches(&summary_entries, &query_tokens);
                prompt_lines.push("--- Conversation summaries ---".to_string());
                prompt_lines.push(format_summary_entries(&summary_entries));
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
            let blocks = crate::services::memories::parse_memory_blocks(trimmed);
            if let Some(query) = last_user_message.clone() {
                if let Some(memory_context) = build_memory_context(&blocks, &query) {
                    context_usage.memories_used = memory_context.count;
                    prompt_lines.push("--- Memories ---".to_string());
                    prompt_lines.push(memory_context.content);
                    prompt_lines.push(
                        "Use the memories above as persistent user facts. Prefer matching context tags and ignore low-confidence items unless confirmed."
                            .to_string(),
                    );
                }
            }
        }
    }
    if let (Some(query), Some(intent)) = (last_user_message.clone(), query_intent) {
        let enriched_query = enrich_query_with_context(&query, &snapshot.chat_history);
        let request = obsidian::ObsidianContextRequest {
            vault_path: &snapshot.connect_obsidian_vault,
            query: &enriched_query,
            intent,
        };
        if let Ok(Some(obsidian_context)) = obsidian::build_obsidian_context(request) {
            context_usage.notes_used = obsidian_context.count;
            let wants_full_display = query_wants_full_note_display(&enriched_query);
            if wants_full_display {
                prompt_lines.push("--- Full Note Content ---".to_string());
                prompt_lines.push(
                    "Share the note content below with the user. Include relevant details."
                        .to_string(),
                );
            } else {
                prompt_lines.push("--- Obsidian Notes ---".to_string());
                prompt_lines.push(
                    "Reference information from the notes below when answering about the user's notes."
                        .to_string(),
                );
            }
            prompt_lines.push(obsidian_context.content);
        }
    }
    let has_context_usage = context_usage.notes_used > 0
        || context_usage.history_used > 0
        || context_usage.memories_used > 0;

    let mut pending_search_notice: Option<String> = None;
    if let (Some(query), Some(intent)) = (last_user_message.clone(), query_intent) {
        let search_context = search::SearchContext::new(snapshot.connect_brave_key.clone());
        pending_search_notice = search::enrich_prompt_with_search_snapshot(
            &search_context,
            &mut prompt_lines,
            search::SearchSnapshotRequest { query: &query, intent },
        );
    }

    let has_search_context = prompt_lines
        .iter()
        .any(|line| line.starts_with("Brave search results for"));
    
    if snapshot.personality_enabled {
        if let Some(text) = &personality_text {
            if !text.trim().is_empty() {
                prompt_lines.push(text.trim().to_string());
            }
        }
    }
    
    let merged_prompt = prompt_lines.join("\n\n");
    let system_context = merged_prompt.clone();
    let mut messages = vec![AgentChatMessage::system(merged_prompt)];
    for chat_message in &snapshot.chat_history {
        if chat_message.role == MessageRole::User {
            messages.push(AgentChatMessage::user(&chat_message.content));
        } else if chat_message.role == MessageRole::Assistant {
            messages.push(AgentChatMessage::assistant(&chat_message.content));
        }
    }

    ChatBuildResultWithUsage {
        messages,
        system_context,
        should_verify: has_context_usage || has_search_context,
        context_usage: if has_context_usage {
            Some(context_usage)
        } else {
            None
        },
        pending_search_notice,
    }
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

fn model_name_matches_case_insensitive(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}
