use super::AgentEvent;
use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, TextInput};
use crate::storage::ConversationData;
use chrono::Local;
use color_eyre::Result;

impl App {
    fn parse_summary_pair(summary: &str) -> (String, String) {
        let mut short = String::new();
        let mut detailed = String::new();
        for line in summary.lines() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("Short:") {
                short = value.trim().to_string();
            } else if let Some(value) = trimmed.strip_prefix("Detailed:") {
                detailed = value.trim().to_string();
            }
        }

        if short.is_empty() {
            if let Some(first_line) = summary
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
            {
                short = first_line.to_string();
            } else {
                short = "Conversation".to_string();
            }
        }
        if detailed.is_empty() {
            detailed = short.clone();
        }

        if short.trim().eq_ignore_ascii_case("conversation") && detailed.len() > 20 {
            short = detailed.clone();
        }

        short = Self::clamp_summary_words(&short, 12);

        (short, detailed)
    }

    fn clamp_summary_words(summary: &str, max_words: usize) -> String {
        let words: Vec<&str> = summary.split_whitespace().collect();
        if words.len() <= max_words {
            return summary.to_string();
        }
        words[..max_words].join(" ")
    }
    fn model_source_for(&self, agent_name: &str, model_name: &str) -> Option<crate::app::ModelSource> {
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
        self.mode = AppMode::Chat;

        if let Err(e) = manager.check_agent_ready(&agent) {
            self.chat_history.push(ChatMessage {
                role: MessageRole::System,
                content: format!("⚠️  {} agent not ready: {}", agent_name, e),
                timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
            });
        }
        Ok(())
    }

    /// Adds a user message to the chat history with timestamp
    fn add_user_message_to_history(&mut self, message_content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            content: message_content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
        });
    }

    /// Extracts agent chat dependencies (agent, manager, channel)
    fn get_agent_chat_dependencies(
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
    fn build_agent_messages(&mut self, system_prompt: &str) -> Vec<AgentChatMessage> {
        if self.personality_enabled && self.personality_text.is_none() {
            if let Ok(text) = crate::services::personality::read_personality() {
                if !text.trim().is_empty() {
                    self.personality_text = Some(text);
                }
            }
            if self.personality_name.is_none() {
                self.personality_name = crate::services::personality::personality_name().ok();
            }
        }

        let mut prompt_lines = vec![system_prompt.to_string()];
        if self.personality_enabled {
            if let Some(text) = &self.personality_text {
                if !text.trim().is_empty() {
                    prompt_lines.push(text.trim().to_string());
                }
            }
            if let Some(name) = self.personality_name.as_deref() {
                prompt_lines.push(format!(
                    "Your name is {}. When asked who you are, reply that you are {}.",
                    name, name
                ));
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
    fn spawn_agent_chat_thread(
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

    pub fn send_chat_message(&mut self) -> Result<()> {
        if self.chat_input.is_empty() {
            return Ok(());
        }

        if self.handle_convert_command()? {
            return Ok(());
        }

        if self.handle_download_command()? {
            return Ok(());
        }

        let user_message = self.chat_input.content().to_string();
        self.chat_input.clear();
        self.reset_chat_scroll();

        self.add_user_message_to_history(&user_message);
        self.is_loading = true;

        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;
        let messages = self.build_agent_messages(&agent.system_prompt);

        Self::spawn_agent_chat_thread(agent, manager, messages, agent_tx);

        Ok(())
    }

    fn handle_convert_command(&mut self) -> Result<bool> {
        let content = self.chat_input.content().trim().to_string();
        if !(content == "convert" || content.starts_with("convert ")) {
            return Ok(false);
        }

        let mut parts = content.splitn(3, ' ');
        let _ = parts.next();
        let format = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").trim();

        self.chat_input.clear();
        self.reset_chat_scroll();

        if format.is_empty() || path.is_empty() {
            self.add_system_message("Usage: convert <format> <path>");
            return Ok(true);
        }

        let tx = self.agent_tx.clone();
        self.conversion_active = true;
        self.conversion_frame = 0;
        self.last_conversion_tick = None;

        let input_path = path.to_string();
        let format_copy = format.clone();
        std::thread::spawn(move || {
            let result = crate::services::convert::convert_file(&input_path, &format_copy);
            if let Some(tx) = tx {
                if let Err(error) = result {
                    let _ = tx.send(AgentEvent::SystemMessage(format!(
                        "Conversion failed: {}",
                        error
                    )));
                }
                let _ = tx.send(AgentEvent::ConversionFinished);
            }
        });

        Ok(true)
    }

    fn handle_download_command(&mut self) -> Result<bool> {
        let content = self.chat_input.content().trim().to_string();
        if !(content == "download" || content.starts_with("download ")) {
            return Ok(false);
        }

        let url = content.trim_start_matches("download").trim().to_string();
        self.chat_input.clear();
        self.reset_chat_scroll();

        if url.is_empty() {
            self.add_system_message("Usage: download <url>");
            return Ok(true);
        }

        let tx = self.agent_tx.clone();
        self.download_active = true;
        self.download_frame = 0;
        self.last_download_tick = None;
        self.download_progress = None;

        std::thread::spawn(move || {
            let result = crate::services::link_download::download_video_with_progress(
                &url,
                |progress| {
                    if let Some(tx) = &tx {
                        let _ = tx.send(AgentEvent::DownloadProgress(progress));
                    }
                },
            );
            if let Some(tx) = tx {
                if let Err(error) = result {
                    let _ = tx.send(AgentEvent::SystemMessage(format!(
                        "Download failed: {}",
                        error
                    )));
                }
                let _ = tx.send(AgentEvent::DownloadFinished);
            }
        });

        Ok(true)
    }

    pub fn check_agent_response(&mut self) {
        if let Some(rx) = &self.agent_rx
            && let Ok(event) = rx.try_recv()
        {
            match event {
                AgentEvent::Response(response) => {
                    self.is_loading = false;
                    self.last_response = Some(response.clone());
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: response.clone(),
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name: self.personality_name.clone(),
                    });

                    // Auto-scroll to bottom if enabled
                    if self.chat_auto_scroll {
                        self.chat_scroll_offset = 0;
                    }

                    if self.auto_tts_enabled
                        && let Some(tts) = &self.tts_service
                        && tts.is_configured()
                    {
                        let _ = tts.speak_text(&response);
                    }

                }
                AgentEvent::Error(error) => {
                    self.is_loading = false;
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!("Error: {}", error),
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name: None,
                    });

                    // Auto-scroll to bottom if enabled
                    if self.chat_auto_scroll {
                        self.chat_scroll_offset = 0;
                    }
                }
                AgentEvent::SummaryGenerated(summary) => {
                    self.is_generating_summary = false;
                    self.summary_active = false;
                    self.summary_frame = 0;
                    self.last_summary_tick = None;

                    if let Some(storage) = &self.storage {
                        let agent_name = self
                            .current_agent
                            .as_ref()
                            .map_or("unknown", |agent| agent.name.as_str());

                        let messages: Vec<(String, String, String)> = self
                            .chat_history
                            .iter()
                            .map(|message| {
                                let role = match message.role {
                                    MessageRole::User => "User",
                                    MessageRole::Assistant => "Assistant",
                                    MessageRole::System => "System",
                                };
                                (
                                    role.to_string(),
                                    message.content.clone(),
                                    message.timestamp.clone(),
                                )
                            })
                            .collect();

                        let (short_summary, detailed_summary) =
                            Self::parse_summary_pair(&summary);
                        if let Some(conversation_id) = self.current_conversation_id {
                            let _ = storage.update_conversation(
                                conversation_id,
                                &short_summary,
                                &detailed_summary,
                                &messages,
                            );
                        } else {
                            let conversation_data = ConversationData::new(agent_name, &messages)
                                .with_summary(&short_summary)
                                .with_detailed_summary(&detailed_summary);

                            if let Ok(conversation_id) = storage.save_conversation(conversation_data) {
                                self.current_conversation_id = Some(conversation_id);
                            }
                        }
                    }
                    self.open_history();
                }
                AgentEvent::SystemMessage(message) => {
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::System,
                        content: message,
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name: None,
                    });
                }
                AgentEvent::DownloadFinished => {
                    self.download_active = false;
                    self.download_frame = 0;
                    self.last_download_tick = None;
                    self.download_progress = None;
                }
                AgentEvent::DownloadProgress(progress) => {
                    self.download_progress = Some(progress);
                }
                AgentEvent::ConversionFinished => {
                    self.conversion_active = false;
                    self.conversion_frame = 0;
                    self.last_conversion_tick = None;
                }
            }
        }
    }

    pub fn add_chat_input_char(&mut self, character: char) {
        self.chat_input.add_char(character);
    }

    pub fn remove_chat_input_char(&mut self) {
        self.chat_input.remove_char();
    }

    /// Builds conversation context from recent messages for summary generation
    fn build_summary_context(&self) -> String {
        self.chat_history
            .iter()
            .filter(|message| message.role != MessageRole::System)
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Spawns a background thread to generate conversation summary
    fn spawn_summary_generation_thread(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        context: String,
        agent_tx: std::sync::mpsc::Sender<AgentEvent>,
    ) {
        let summary_prompt = format!(
            "Generate two summaries for this conversation.\n\
Short: 7-12 words.\n\
Detailed: 2-3 sentences.\n\
Return only two lines in this exact format:\n\
Short: <summary>\n\
Detailed: <summary>\n\n\
Conversation: {}",
            context.chars().take(400).collect::<String>()
        );

        std::thread::spawn(move || {
            let messages = vec![
                AgentChatMessage::system(
                    "You create short and detailed conversation summaries. Follow the requested format exactly.",
                ),
                AgentChatMessage::user(&summary_prompt),
            ];
            let response = match manager.chat(&agent, &messages) {
                Ok(text) => text,
                Err(_) => "Short: Conversation\nDetailed: Conversation".to_string(),
            };
            let (short, detailed) = Self::parse_summary_pair(&response);
            let payload = format!("{}\n{}", short, detailed);
            let _ = agent_tx.send(AgentEvent::SummaryGenerated(payload));
        });
    }

    pub fn exit_chat_to_history(&mut self) -> Result<()> {
        if self.chat_history.is_empty() {
            self.open_history();
            return Ok(());
        }

        self.is_generating_summary = true;
        self.summary_active = true;

        let context = self.build_summary_context();
        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;

        Self::spawn_summary_generation_thread(agent, manager, context, agent_tx);

        Ok(())
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
        });
    }

    pub fn speak_last_response(&self) -> Result<()> {
        let response = self
            .last_response
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("No response to speak"))?;
        let tts = self
            .tts_service
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("TTS service not initialized"))?;

        if tts.is_playing() {
            tts.stop();
        } else {
            tts.speak_text(response)?;
        }
        Ok(())
    }
}
