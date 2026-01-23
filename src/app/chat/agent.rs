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
    tokenize_query,
};
use crate::app::chat::agent::intent::{classify_query_with_model, IntentModelContext, QueryIntent};
use chrono::Local;
use color_eyre::Result;
use std::sync::OnceLock;

/// Global runtime for async storage operations (initialized once, reused)
static ASYNC_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn get_async_runtime() -> Option<&'static tokio::runtime::Runtime> {
    if ASYNC_RUNTIME.get().is_none() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        let _ = ASYNC_RUNTIME.set(runtime);
    }
    ASYNC_RUNTIME.get()
}

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
                // Only load personality text if explicitly selected (not None)
                if let Some(selected_name) = &self.personality_name {
                    if let Ok(text) = crate::services::personality::read_personality(selected_name)
                        && !text.trim().is_empty()
                    {
                        self.personality_text = Some(text);
                    } else {
                        self.personality_text = None;
                    }
                } else {
                    self.personality_text = None;
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
                .is_some_and(|model_name| self.is_gab_model_name(model_name))
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
                        if let Ok(verified) = manager.chat(&agent, &verify_messages)
                            && !verified.trim().is_empty()
                        {
                            let context_usage_for_verify = context_usage.clone();
                            let _ = agent_tx.send(AgentEvent::ResponseWithContext {
                                response: verified,
                                context_usage: context_usage_for_verify,
                            });
                            return;
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
        "give me the note",
        "give me detailed",
        "detailed note",
        "the whole note",
        "full note",
        "entire note",
        "complete note",
        "everything",
        "all of it",
        "show me everything",
        "show me all",
        "show me the note",
        "show the note",
        "see the note",
        "read the note",
        "give me everything",
        "paste it",
        "paste that",
        "copy it",
        "copy that",
        "more detail",
        "more details",
        "in detail",
        "in full",
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
    /// Pre-retrieved messages (retrieved before thread spawn while App storage is accessible)
    pub pre_retrieved_messages: Vec<crate::storage::RetrievedMessage>,
    /// Cached Obsidian notes from previous query (for follow-up questions)
    pub cached_obsidian_notes: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
}

pub(crate) struct ChatBuildResultWithUsage {
    pub messages: Vec<AgentChatMessage>,
    pub system_context: String,
    pub should_verify: bool,
    pub context_usage: Option<ContextUsage>,
    pub pending_search_notice: Option<String>,
    pub forced_response: Option<String>,
    pub notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
}

pub(crate) fn build_agent_messages_from_snapshot(
    snapshot: ChatBuildSnapshot,
    agent: &crate::agents::Agent,
    manager: &crate::agents::AgentManager,
) -> ChatBuildResultWithUsage {
    let mut personality_text = snapshot.personality_text.clone();
    if snapshot.personality_enabled && personality_text.is_none() {
        // Only load personality text if explicitly selected (not None)
        if let Some(selected_name) = &snapshot.personality_name
            && let Ok(text) = crate::services::personality::read_personality(selected_name)
            && !text.trim().is_empty()
        {
            personality_text = Some(text);
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
    let mut has_memory_context = false;
    let mut forced_response: Option<String> = None;
    let mut notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)> = None;
    
    // Use global runtime for async storage operations (for summaries/obsidian only)
    let runtime = get_async_runtime();
    let storage = runtime.and_then(|rt| {
        rt.block_on(async {
            crate::storage::StorageManager::new().await.ok()
        })
    });
    
    let routing_agent = manager.get_agent("routing").cloned();
    let is_profile_query = last_user_message
        .as_ref()
        .is_some_and(|query| crate::services::retrieval::is_profile_query(query));
    
    // Use pre-retrieved messages (retrieved before thread spawn while App storage was accessible)
    if !snapshot.pre_retrieved_messages.is_empty() {
        context_usage.memories_used = snapshot.pre_retrieved_messages.len();
        has_memory_context = true;
        
        // For profile queries: Two-stage LLM approach to prevent hallucination
        // Stage 1: Plain fact summarization (no personality)
        // Stage 2: Add personality to the plain summary
        if is_profile_query {
            let mut extracted_facts: Vec<String> = Vec::new();
            
            for msg in &snapshot.pre_retrieved_messages {
                // Only user statements (not questions or assistant responses)
                if msg.role == "User" && !msg.content.contains('?') {
                    extracted_facts.push(msg.content.clone());
                }
            }
            
            // Deduplicate
            extracted_facts.sort();
            extracted_facts.dedup();
            
            if !extracted_facts.is_empty() {
                let facts_text = extracted_facts.iter()
                    .map(|f| format!("• {}", f))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                // Stage 1: Get plain summary (no personality)
                let stage1_messages = vec![
                    AgentChatMessage {
                        role: crate::agents::MessageRole::System,
                        content: "You are a factual summarizer. Simply list what the user has told you, nothing else.".to_string(),
                        images: vec![],
                    },
                    AgentChatMessage {
                        role: crate::agents::MessageRole::User,
                        content: format!(
                            "The user asked what you remember about them. Here are their statements:\n\n{}\n\n\
                             Summarize these facts in second person (e.g. 'You like...'). \
                             Do NOT add new facts. Do NOT ask questions. Keep it plain and direct.",
                            facts_text
                        ),
                        images: vec![],
                    },
                ];
                
                if let Ok(plain_summary) = manager.chat(agent, &stage1_messages) {
                    // Stage 2: Add personality to the plain summary
                    let stage2_messages = vec![
                        AgentChatMessage {
                            role: crate::agents::MessageRole::System,
                            content: agent.system_prompt.clone(),
                            images: vec![],
                        },
                        AgentChatMessage {
                            role: crate::agents::MessageRole::User,
                            content: format!(
                                "Add your personality style to this factual summary (keep the facts unchanged):\n\n{}",
                                plain_summary.trim()
                            ),
                            images: vec![],
                        },
                    ];
                    
                    if let Ok(personality_response) = manager.chat(agent, &stage2_messages) {
                        forced_response = Some(personality_response);
                    } else {
                        // Fallback to plain summary if stage 2 fails
                        forced_response = Some(plain_summary);
                    }
                } else {
                    // Fallback to deterministic format if stage 1 fails
                    forced_response = Some("I don't have any information about your preferences yet.".to_string());
                }
            } else {
                // No facts found (only questions/assistant messages were retrieved)
                forced_response = Some("I don't have any information about your preferences yet.".to_string());
            }
        } else {
            // Non-profile queries: use full context as before
            prompt_lines.push("--- Relevant Past Messages ---".to_string());
            for msg in &snapshot.pre_retrieved_messages {
                prompt_lines.push(format!(
                    "[{}] {}: {}",
                    msg.timestamp, msg.role, msg.content
                ));
            }
            prompt_lines.push(
                "Use the relevant messages above for context when answering."
                    .to_string()
            );
        }
    }
    
    if let Some(query) = last_user_message.clone() {
        let query_tokens = tokenize_query(&query);
        let intent_context = IntentModelContext {
            manager,
            routing_agent: routing_agent.as_ref(),
            fallback_agent: agent,
        };
        query_intent = Some(classify_query_with_model(&query, intent_context));
        
        if let Ok(summary_entries) = build_conversation_summary_entries(storage.as_ref(), &query)
            && !summary_entries.is_empty()
        {
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

    if is_profile_query && context_usage.memories_used == 0 {
        forced_response = Some("I don't have any information about your preferences yet.".to_string());
    }

    if forced_response.is_some() {
        return ChatBuildResultWithUsage {
            messages: Vec::new(),
            system_context: prompt_lines.join("\n\n"),
            should_verify: false,
            context_usage: None,
            pending_search_notice: None,
            forced_response,
            notes_to_cache: None,
        };
    }
    if let (Some(query), Some(intent)) = (last_user_message.clone(), query_intent) {
        let enriched_query = enrich_query_with_context(&query, &snapshot.chat_history);
        let wants_full_display = query_wants_full_note_display(&enriched_query);
        
        // Check if this is a follow-up about cached notes
        let is_notes_follow_up = wants_full_display 
            && snapshot.cached_obsidian_notes.is_some();
        
        if is_notes_follow_up {
            // Use cached notes for follow-up questions
            if let Some((_, cached_notes)) = &snapshot.cached_obsidian_notes {
                context_usage.notes_used = cached_notes.len();
                prompt_lines.push("--- Full Note Content ---".to_string());
                prompt_lines.push(
                    "Share the note content below with the user. Include relevant details."
                        .to_string(),
                );
                
                // Include full cached note content
                for note in cached_notes {
                    prompt_lines.push(format!("## {}", note.title));
                    prompt_lines.push(note.snippet.clone());
                    prompt_lines.push("".to_string());
                }
            }
        } else {
            // Fetch fresh notes
            let request = obsidian::ObsidianContextRequest {
                vault_path: &snapshot.connect_obsidian_vault,
                query: &enriched_query,
                intent,
            };
            if let Ok(Some(obsidian_context)) = obsidian::build_obsidian_context(request) {
                context_usage.notes_used = obsidian_context.count;
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
                
                // Cache notes for follow-up questions
                if !obsidian_context.raw_notes.is_empty() {
                    notes_to_cache = Some((query.clone(), obsidian_context.raw_notes));
                }
            }
        }
    }
    let has_context_usage = context_usage.notes_used > 0
        || context_usage.history_used > 0
        || context_usage.memories_used > 0;

    let mut pending_search_notice: Option<String> = None;
    if !is_profile_query
        && !has_memory_context
        && let (Some(query), Some(intent)) = (last_user_message.clone(), query_intent)
    {
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
    
    if let Ok(Some(identity_prompt)) = crate::services::identity::build_identity_prompt() {
        prompt_lines.push(identity_prompt);
    }
    if snapshot.personality_enabled
        && let Some(text) = &personality_text
        && !text.trim().is_empty()
    {
        prompt_lines.push(text.trim().to_string());
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
        forced_response,
        notes_to_cache,
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
