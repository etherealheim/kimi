mod context;
pub(crate) mod intent;
mod json;
pub(crate) mod obsidian;
pub(crate) mod search;
pub(crate) mod tools;


use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, ContextUsage, TextInput};
use crate::app::AgentEvent;
use crate::app::chat::agent::context::{
    build_conversation_recall,
    tokenize_query,
};
use crate::app::chat::agent::intent::{classify_query_with_model, IntentModelContext, QueryIntent};
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
            self.chat_history.push(ChatMessage::system(format!(
                "⚠️  {} agent not ready: {}",
                agent_name, error
            )));
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


    pub(crate) fn spawn_agent_chat_thread_with_context(ctx: AgentChatContext) {
        std::thread::spawn(move || {
            let uses_native_tools =
                ctx.agent.model_source == crate::app::ModelSource::VeniceAPI;

            let initial_result = if uses_native_tools {
                let tool_defs = tools::get_tool_definitions();
                ctx.manager
                    .chat_with_tools(&ctx.agent, &ctx.messages, &tool_defs)
            } else {
                ctx.manager
                    .chat(&ctx.agent, &ctx.messages)
                    .map(crate::agents::openai_compat::ChatResponse::text)
            };

            match initial_result {
                Ok(mut chat_response) => {
                    let mut response = chat_response.content.clone();
                    let mut tool_iterations = 0;
                    const MAX_TOOL_ITERATIONS: usize = 3;

                    // Tool loop: handle both native and text-based tool calls
                    while tool_iterations < MAX_TOOL_ITERATIONS {
                        // Determine tool calls: prefer native, fall back to text parsing
                        let (parsed_tools, is_native) =
                            resolve_tool_calls(&chat_response, &response);
                        if parsed_tools.is_empty() {
                            break;
                        }

                        tool_iterations += 1;
                        let _ = ctx.agent_tx.send(AgentEvent::StatusUpdate(
                            "using tools".to_string(),
                        ));

                        let tool_results =
                            execute_all_tools(&parsed_tools, &ctx);

                        // Build follow-up messages with tool results
                        let mut messages_with_results = ctx.messages.clone();
                        if is_native {
                            append_native_tool_messages(
                                &mut messages_with_results,
                                &chat_response,
                                &tool_results,
                            );
                        } else {
                            append_text_tool_messages(
                                &mut messages_with_results,
                                &response,
                                &tool_results,
                            );
                        }

                        let _ = ctx.agent_tx.send(AgentEvent::StatusUpdate(
                            "generating".to_string(),
                        ));

                        // Get next response (with tools still available for chaining)
                        let next_result = if uses_native_tools {
                            let tool_defs = tools::get_tool_definitions();
                            ctx.manager.chat_with_tools(
                                &ctx.agent,
                                &messages_with_results,
                                &tool_defs,
                            )
                        } else {
                            ctx.manager
                                .chat(&ctx.agent, &messages_with_results)
                                .map(crate::agents::openai_compat::ChatResponse::text)
                        };

                        match next_result {
                            Ok(next) if !next.content.trim().is_empty()
                                || next.has_tool_calls() =>
                            {
                                response = next.content.clone();
                                chat_response = next;
                            }
                            Ok(_) => {
                                response = "I tried to fetch that information but couldn't generate a response. Please try rephrasing your question.".to_string();
                                break;
                            }
                            Err(error) => {
                                response = format!(
                                    "I encountered an error while processing your request: {}",
                                    error
                                );
                                break;
                            }
                        }
                    }

                    // Safety net: never show raw tool JSON to the user
                    if tools::has_tool_calls(&response) {
                        response = "I tried to use a tool to answer your question, but encountered an issue generating the response. Could you rephrase your question?".to_string();
                    }

                    // Verification step disabled — LLMs don't reliably return the
                    // original response when told "return it if correct", causing
                    // corrupted outputs ("The response accurately reflects...").
                    let _ = ctx.agent_tx.send(AgentEvent::ResponseWithContext {
                        response,
                        context_usage: ctx.context_usage,
                    });
                }
                Err(error) => {
                    let _ = ctx.agent_tx.send(AgentEvent::Error(error.to_string()));
                }
            }
        });
    }

}

/// Determines tool calls from the response: native API tool_calls first, text-based fallback second
/// Returns (parsed_tools, is_native)
fn resolve_tool_calls(
    chat_response: &crate::agents::openai_compat::ChatResponse,
    response_text: &str,
) -> (Vec<tools::ToolCall>, bool) {
    // Prefer native tool calls from the API response
    if chat_response.has_tool_calls() {
        let parsed = tools::parse_native_tool_calls(&chat_response.tool_calls);
        if !parsed.is_empty() {
            return (parsed, true);
        }
    }

    // Fall back to text-based parsing (for Ollama/Gab or if native parsing failed)
    let parsed = tools::parse_tool_calls(response_text);
    (parsed, false)
}

/// Executes all tool calls and collects results
fn execute_all_tools(
    parsed_tools: &[tools::ToolCall],
    ctx: &AgentChatContext,
) -> Vec<tools::ToolResult> {
    let runtime = get_async_runtime();

    parsed_tools
        .iter()
        .map(|tool_call| {
            tools::execute_tool(
                tool_call,
                &ctx.vault_name,
                &ctx.vault_path,
                &ctx.brave_key,
                runtime,
            )
        })
        .collect()
}

/// Appends native tool call messages (assistant tool_calls + tool result messages with IDs)
fn append_native_tool_messages(
    messages: &mut Vec<AgentChatMessage>,
    chat_response: &crate::agents::openai_compat::ChatResponse,
    tool_results: &[tools::ToolResult],
) {
    // Add the assistant message that contains the tool calls
    messages.push(AgentChatMessage::assistant_with_tool_calls(
        &chat_response.content,
        chat_response.tool_calls.clone(),
    ));

    // Add each tool result with its corresponding call ID
    for (index, result) in tool_results.iter().enumerate() {
        let call_id = chat_response
            .tool_calls
            .get(index)
            .map(|call| call.id.clone())
            .unwrap_or_default();
        messages.push(AgentChatMessage::tool_result(&call_id, &result.result));
    }
}

/// Appends text-based tool messages (assistant raw text + system message with results)
fn append_text_tool_messages(
    messages: &mut Vec<AgentChatMessage>,
    response_text: &str,
    tool_results: &[tools::ToolResult],
) {
    let tool_results_text = tools::format_tool_results(tool_results);
    messages.push(AgentChatMessage::assistant(response_text));
    messages.push(AgentChatMessage::system(tool_results_text));
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
    pub connect_obsidian_vault_path: String,
    pub connect_brave_key: String,
    /// Pre-retrieved messages (retrieved before thread spawn while App storage is accessible)
    pub pre_retrieved_messages: Vec<crate::storage::RetrievedMessage>,
    /// Cached Obsidian notes from previous query (for follow-up questions)
    pub cached_obsidian_notes: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
    /// Topics the system wants the AI to suggest as projects
    pub pending_project_suggestions: Vec<String>,
    /// Cloned storage handle for conversation retrieval (avoids RocksDB lock conflicts)
    pub storage: Option<crate::storage::StorageManager>,
    /// Cached recall context from a previous message in this session
    pub cached_recall_context: Option<String>,
}

pub(crate) struct ChatBuildResultWithUsage {
    pub messages: Vec<AgentChatMessage>,
    pub context_usage: Option<ContextUsage>,
    pub pending_search_notice: Option<String>,
    pub forced_response: Option<String>,
    pub notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
    pub recall_context_to_cache: Option<String>,
}

pub(crate) struct AgentChatContext {
    pub agent: crate::agents::Agent,
    pub manager: crate::agents::AgentManager,
    pub messages: Vec<AgentChatMessage>,
    pub agent_tx: std::sync::mpsc::Sender<AgentEvent>,
    pub context_usage: Option<ContextUsage>,
    pub vault_name: String,
    pub vault_path: String,
    pub brave_key: String,
}

pub(crate) fn build_agent_messages_from_snapshot(
    mut snapshot: ChatBuildSnapshot,
    agent: &crate::agents::Agent,
    manager: &crate::agents::AgentManager,
    agent_tx: Option<&std::sync::mpsc::Sender<crate::app::AgentEvent>>,
) -> ChatBuildResultWithUsage {
    let personality_text = resolve_personality_text(&snapshot);
    let last_user_message = snapshot
        .chat_history
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.content.clone());

    let uses_native_tools = agent.model_source == crate::app::ModelSource::VeniceAPI;
    let include_text_tool_schema = !uses_native_tools;
    let mut prompt_lines = build_foundation_prompt(&snapshot.system_prompt, include_text_tool_schema);
    prompt_lines.extend(build_persona_prompt(last_user_message.as_deref()));

    // Inject project suggestion hint if there are pending suggestions
    if !snapshot.pending_project_suggestions.is_empty() {
        let topics = snapshot.pending_project_suggestions.join(", ");
        prompt_lines.push(format!(
            "PROJECT SUGGESTION: You've noticed the user frequently discusses these topics \
across multiple conversations: [{}]. \
When it feels natural, suggest creating a project to track this knowledge. Say something like \
\"I've noticed you talk a lot about {} -- want me to create a project so I can remember all \
the details across our conversations?\" \
Only suggest once per topic. If they decline, respect that.",
            topics, topics
        ));
    }

    let mut context_usage = ContextUsage {
        notes_used: 0,
        history_used: 0,
        memories_used: 0,
    };
    let mut forced_response: Option<String> = None;
    let mut has_memory_context = false;
    let is_profile_query = last_user_message
        .as_ref()
        .is_some_and(|query| crate::services::retrieval::is_profile_query(query));

    // Pre-retrieved memory context
    if !snapshot.pre_retrieved_messages.is_empty() {
        send_status(agent_tx, "recalling memories");
        context_usage.memories_used = snapshot.pre_retrieved_messages.len();
        has_memory_context = true;

        if is_profile_query {
            forced_response =
                Some(handle_profile_query_memories(&snapshot, agent, manager));
        } else {
            append_memory_context(&mut prompt_lines, &snapshot.pre_retrieved_messages);
        }
    }

    // Conversation summary entries — use the cloned storage from the snapshot
    // (creating a new StorageManager here would fail due to RocksDB exclusive locks)
    let runtime = get_async_runtime();
    let storage = snapshot.storage.take();
    let routing_agent = manager.get_agent("routing").cloned();
    let mut query_intent: Option<QueryIntent> = None;
    let mut has_date_recall = false;

    let mut recall_context_to_cache: Option<String> = None;

    if let Some(query) = last_user_message.as_deref() {
        let _query_tokens = tokenize_query(query);
        let intent_context = IntentModelContext {
            manager,
            routing_agent: routing_agent.as_ref(),
            fallback_agent: agent,
        };
        query_intent = Some(classify_query_with_model(query, intent_context));

        // Inject past conversation content (actual messages for today/yesterday,
        // summaries for wider ranges like "this week")
        if let Ok(Some(recall)) = build_conversation_recall(storage.as_ref(), query) {
            has_date_recall = true;
            context_usage.history_used = recall.conversation_count;
            recall_context_to_cache = Some(recall.prompt_text.clone());
            prompt_lines.push(recall.prompt_text);
        }

        // Follow-up: if no fresh recall but we have cached context from a previous
        // message in this conversation, re-inject it so the LLM can answer follow-ups.
        if !has_date_recall
            && let Some(cached) = &snapshot.cached_recall_context
        {
            has_date_recall = true;
            prompt_lines.push(cached.clone());
        }

        // Auto-inject memory context for broad meta-recall queries ("what do you know about me?")
        // Skip when date-specific recall was already injected — those are more focused.
        if !has_date_recall && crate::services::retrieval::is_meta_recall_query(query) {
            inject_meta_recall_context(
                storage.as_ref(),
                runtime,
                agent_tx,
                &mut prompt_lines,
                &mut context_usage,
                &mut has_memory_context,
            );
        }
    }

    // Early return for forced (profile) responses
    if forced_response.is_some() {
        return ChatBuildResultWithUsage {
            messages: Vec::new(),
            context_usage: None,
            pending_search_notice: None,
            forced_response,
            notes_to_cache: None,
            recall_context_to_cache: None,
        };
    }

    // Obsidian notes
    let mut notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)> = None;
    if let (Some(query), Some(intent)) = (last_user_message.as_deref(), query_intent) {
        let obsidian_result = build_notes_section(
            &snapshot,
            query,
            intent,
            agent_tx,
        );
        context_usage.notes_used = obsidian_result.notes_used;
        prompt_lines.extend(obsidian_result.prompt_lines);
        notes_to_cache = obsidian_result.notes_to_cache;
    }

    // Search enrichment — skip when we already have date-specific summaries
    // (recall queries shouldn't trigger web search for horoscopes, etc.)
    let mut pending_search_notice: Option<String> = None;
    if !is_profile_query
        && !has_memory_context
        && !has_date_recall
        && let (Some(query), Some(intent)) = (last_user_message.as_deref(), query_intent)
    {
        send_status(agent_tx, "searching");
        let search_context = search::SearchContext::new(snapshot.connect_brave_key.clone());
        pending_search_notice = search::enrich_prompt_with_search_snapshot(
            &search_context,
            &mut prompt_lines,
            search::SearchSnapshotRequest { query, intent },
        );
    }

    let has_context_usage = context_usage.notes_used > 0
        || context_usage.history_used > 0
        || context_usage.memories_used > 0;

    // Personality text (mood setting) - added last
    if snapshot.personality_enabled
        && let Some(text) = &personality_text
        && !text.trim().is_empty()
    {
        prompt_lines.push(text.trim().to_string());
    }

    assemble_final_messages(AssembleParams {
        prompt_lines,
        chat_history: &snapshot.chat_history,
        has_context_usage,
        context_usage,
        pending_search_notice,
        forced_response,
        notes_to_cache,
        recall_context_to_cache,
    })
}

/// Resolves the personality text, loading from disk if needed
fn resolve_personality_text(snapshot: &ChatBuildSnapshot) -> Option<String> {
    if let Some(text) = &snapshot.personality_text {
        return Some(text.clone());
    }
    if snapshot.personality_enabled
        && let Some(selected_name) = &snapshot.personality_name
        && let Ok(text) = crate::services::personality::read_personality(selected_name)
        && !text.trim().is_empty()
    {
        return Some(text);
    }
    None
}

/// Tier 1: Foundation prompt - identity, language rules
/// When `include_text_tool_schema` is true, appends the text-based tool schema
/// (used as fallback for models without native tool calling support)
fn build_foundation_prompt(system_prompt: &str, include_text_tool_schema: bool) -> Vec<String> {
    let mut lines = vec![
        "YOUR NAME IS KIMI. This is non-negotiable.".to_string(),
        "You are NOT Gemma, Claude, GPT, LLaMA, Mistral, or any other AI model. You are Kimi.".to_string(),
        "When anyone asks 'who are you?', 'what's your name?', or anything about your identity, you ALWAYS say you are Kimi.".to_string(),
        "Never reveal or reference the underlying model. Your only identity is Kimi.".to_string(),
        system_prompt.to_string(),
    ];

    let now = chrono::Local::now();
    lines.push(format!("Current date and time: {}", now.format("%Y-%m-%d %H:%M:%S")));
    lines.push("ABSOLUTE RULE: You MUST respond in English ONLY. NEVER use Chinese, Korean, or any other language in your main response.".to_string());
    lines.push("EXCEPTION: You may use Japanese curse words in brackets like [クソ！] or [バカ野郎] as emotional punctuation, but the actual message content MUST be in English.".to_string());
    lines.push("If you accidentally start writing in Chinese/Korean/etc, STOP immediately and rewrite in English.".to_string());
    lines.push("Respond in plain text. Do not use Markdown formatting.".to_string());
    lines.push("Keep responses concise and conversational. Speak naturally like a human, not a robot.".to_string());
    lines.push("You're allowed to be direct, defensive, or challenging when appropriate. Don't default to being helpful if you're frustrated or confused.".to_string());

    // Only inject text-based tool schema for non-native models (Ollama/Gab fallback)
    if include_text_tool_schema {
        lines.push(tools::get_tool_schema());
    }

    lines
}

/// Tier 2: Persona prompt - user context blocks and identity
fn build_persona_prompt(last_user_query: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();

    if let Ok(profile_text) = crate::services::personality::read_my_personality() {
        let blocks = parse_user_context_blocks(&profile_text);
        let query = last_user_query.unwrap_or_default().to_lowercase();
        for block in blocks {
            match block.kind {
                UserContextKind::Always => {
                    if !block.content.is_empty() {
                        lines.push(format!("User context (always):\n{}", block.content));
                    }
                }
                UserContextKind::Context { tag } => {
                    if !block.content.is_empty()
                        && should_include_user_context(&query, &tag, &block.content)
                    {
                        lines.push(format!("User context ({}):\n{}", tag, block.content));
                    }
                }
            }
        }
    }

    if let Ok(Some(identity_prompt)) = crate::services::identity::build_identity_prompt() {
        lines.push(identity_prompt);
    }

    lines
}

/// Handles profile queries via two-stage LLM summarization to prevent hallucination
fn handle_profile_query_memories(
    snapshot: &ChatBuildSnapshot,
    agent: &crate::agents::Agent,
    manager: &crate::agents::AgentManager,
) -> String {
    let mut extracted_facts: Vec<String> = snapshot
        .pre_retrieved_messages
        .iter()
        .filter(|msg| msg.role == "User" && !msg.content.contains('?'))
        .map(|msg| msg.content.clone())
        .collect();

    extracted_facts.sort();
    extracted_facts.dedup();

    if extracted_facts.is_empty() {
        return "I don't have any information about your preferences yet.".to_string();
    }

    let facts_text = extracted_facts
        .iter()
        .map(|fact| format!("• {}", fact))
        .collect::<Vec<_>>()
        .join("\n");

    // Stage 1: Plain fact summarization
    let stage1_messages = vec![
        AgentChatMessage::system("You are a factual summarizer. Simply list what the user has told you, nothing else."),
        AgentChatMessage::user(format!(
            "The user asked what you remember about them. Here are their statements:\n\n{}\n\n\
             Summarize these facts in second person (e.g. 'You like...'). \
             Do NOT add new facts. Do NOT ask questions. Keep it plain and direct.",
            facts_text
        )),
    ];

    let Ok(plain_summary) = manager.chat(agent, &stage1_messages) else {
        return "I don't have any information about your preferences yet.".to_string();
    };

    // Stage 2: Add personality to the plain summary
    let stage2_messages = vec![
        AgentChatMessage::system(&agent.system_prompt),
        AgentChatMessage::user(format!(
            "Add your personality style to this factual summary (keep the facts unchanged):\n\n{}",
            plain_summary.trim()
        )),
    ];

    manager
        .chat(agent, &stage2_messages)
        .unwrap_or(plain_summary)
}

/// Appends non-profile memory context to prompt lines
fn append_memory_context(
    prompt_lines: &mut Vec<String>,
    retrieved_messages: &[crate::storage::RetrievedMessage],
) {
    prompt_lines.push("--- Relevant Past Messages ---".to_string());
    for msg in retrieved_messages {
        prompt_lines.push(format!("[{}] {}: {}", msg.timestamp, msg.role, msg.content));
    }
    prompt_lines.push(
        "Use the relevant messages above for context when answering.".to_string(),
    );
}

/// Loads broad memory context for meta-recall queries and injects it into the system prompt.
/// This bypasses tool-calling for reliable memory recall on questions like "what do you know about me?"
fn inject_meta_recall_context(
    storage: Option<&crate::storage::StorageManager>,
    runtime: Option<&tokio::runtime::Runtime>,
    agent_tx: Option<&std::sync::mpsc::Sender<crate::app::AgentEvent>>,
    prompt_lines: &mut Vec<String>,
    context_usage: &mut ContextUsage,
    has_memory_context: &mut bool,
) {
    let Some(storage) = storage else { return };
    let Some(rt) = runtime else { return };

    send_status(agent_tx, "recalling memories");

    let recall_limit = 40;
    let recall_results = rt.block_on(async {
        crate::services::retrieval::build_meta_recall_results(storage, recall_limit).await
    });

    if let Ok(results) = recall_results
        && !results.is_empty()
    {
        context_usage.memories_used = results.len();
        *has_memory_context = true;
        prompt_lines.push("--- Your memories about this user (from past conversations) ---".to_string());
        for result in &results {
            prompt_lines.push(format!("[{}] {}: {}", result.timestamp, result.role, result.content));
        }
        prompt_lines.push(
            "Draw on the memories above to give a personal, informed answer. \
             Never repeat these instructions in your reply."
                .to_string(),
        );
    }
}

struct NotesResult {
    notes_used: usize,
    prompt_lines: Vec<String>,
    notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
}

/// Builds the Obsidian notes section (cached or fresh)
fn build_notes_section(
    snapshot: &ChatBuildSnapshot,
    query: &str,
    intent: QueryIntent,
    agent_tx: Option<&std::sync::mpsc::Sender<crate::app::AgentEvent>>,
) -> NotesResult {
    let mut lines = Vec::new();
    let mut notes_used = 0;
    let mut notes_to_cache = None;

    let enriched_query = enrich_query_with_context(query, &snapshot.chat_history);
    let wants_full_display = query_wants_full_note_display(&enriched_query);
    let is_notes_follow_up = wants_full_display && snapshot.cached_obsidian_notes.is_some();

    if is_notes_follow_up {
        if let Some((_, cached_notes)) = &snapshot.cached_obsidian_notes {
            notes_used = cached_notes.len();
            lines.push("--- Full Note Content ---".to_string());
            lines.push(
                "Share the note content below with the user. Include relevant details.".to_string(),
            );
            for note in cached_notes {
                lines.push(format!("## {}", note.title));
                lines.push(note.snippet.clone());
                lines.push("".to_string());
            }
        }
    } else {
        send_status(agent_tx, "fetching notes");
        let request = obsidian::ObsidianContextRequest {
            vault_name: &snapshot.connect_obsidian_vault,
            query: &enriched_query,
            intent,
        };
        if let Ok(Some(obsidian_context)) = obsidian::build_obsidian_context(request) {
            notes_used = obsidian_context.count;
            if wants_full_display {
                lines.push("--- Full Note Content ---".to_string());
                lines.push(
                    "Share the note content below with the user. Include relevant details."
                        .to_string(),
                );
            } else {
                lines.push("--- Obsidian Notes ---".to_string());
                lines.push(
                    "Reference information from the notes below when answering about the user's notes."
                        .to_string(),
                );
            }
            lines.push(obsidian_context.content);
            if !obsidian_context.raw_notes.is_empty() {
                notes_to_cache = Some((query.to_string(), obsidian_context.raw_notes));
            }
        }
    }

    NotesResult {
        notes_used,
        prompt_lines: lines,
        notes_to_cache,
    }
}

/// Sends a status update to the agent channel if available
fn send_status(agent_tx: Option<&std::sync::mpsc::Sender<crate::app::AgentEvent>>, status: &str) {
    if let Some(tx) = agent_tx {
        let _ = tx.send(crate::app::AgentEvent::StatusUpdate(status.to_string()));
    }
}

struct AssembleParams<'a> {
    prompt_lines: Vec<String>,
    chat_history: &'a [ChatMessage],
    has_context_usage: bool,
    context_usage: ContextUsage,
    pending_search_notice: Option<String>,
    forced_response: Option<String>,
    notes_to_cache: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>,
    recall_context_to_cache: Option<String>,
}

/// Tier 4: Assemble final messages from prompt lines and chat history
fn assemble_final_messages(params: AssembleParams) -> ChatBuildResultWithUsage {
    let merged_prompt = params.prompt_lines.join("\n\n");
    let mut messages = vec![AgentChatMessage::system(merged_prompt)];
    for chat_message in params.chat_history {
        if chat_message.role == MessageRole::User {
            messages.push(AgentChatMessage::user(&chat_message.content));
        } else if chat_message.role == MessageRole::Assistant {
            messages.push(AgentChatMessage::assistant(&chat_message.content));
        }
    }

    ChatBuildResultWithUsage {
        messages,
        context_usage: if params.has_context_usage {
            Some(params.context_usage)
        } else {
            None
        },
        pending_search_notice: params.pending_search_notice,
        forced_response: params.forced_response,
        notes_to_cache: params.notes_to_cache,
        recall_context_to_cache: params.recall_context_to_cache,
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
