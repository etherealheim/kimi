mod context;
mod obsidian;
mod search;
mod verification;

pub(crate) use self::search::should_use_brave_search;

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
use chrono::Local;
use color_eyre::Result;

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
    let mut query_tokens: Vec<String> = Vec::new();
    let storage = crate::storage::StorageManager::new().ok();
    if let Some(query) = last_user_message.clone() {
        query_tokens = tokenize_query(&query);
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
    if let Some(query) = last_user_message.clone() {
        if let Ok(Some(obsidian_context)) = obsidian::build_obsidian_context(
            &snapshot.connect_obsidian_vault,
            &query,
            &query_tokens,
        ) {
            context_usage.notes_used = obsidian_context.count;
            prompt_lines.push(
                "Use the Obsidian notes below to answer questions about the user's notes."
                    .to_string(),
            );
            prompt_lines.push(
                "Do not add or infer information that is not explicitly present in the notes."
                    .to_string(),
            );
            prompt_lines.push(obsidian_context.content);
        }
    }
    let has_context_usage = context_usage.notes_used > 0
        || context_usage.history_used > 0
        || context_usage.memories_used > 0;

    let mut pending_search_notice: Option<String> = None;
    if let Some(query) = last_user_message.clone() {
        let search_context = search::SearchContext::new(
            agent.clone(),
            manager.clone(),
            snapshot.connect_brave_key.clone(),
        );
        pending_search_notice = search::enrich_prompt_with_search_snapshot(
            &search_context,
            &mut prompt_lines,
            &query,
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
