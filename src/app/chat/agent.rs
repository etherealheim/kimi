use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, TextInput};
use crate::app::AgentEvent;
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
        prompt_lines.push("Respond in plain text. Do not use Markdown formatting.".to_string());
        if let Ok(profile_text) = crate::services::personality::read_my_personality() {
            let trimmed_profile = profile_text.trim();
            if !trimmed_profile.is_empty() {
                prompt_lines.push(format!("User context:\n{}", trimmed_profile));
            }
        }
        if self.is_brave_search_enabled {
            if let Some(query) = self.last_user_message_content() {
                if !self.connect_brave_key.trim().is_empty() {
                    match crate::services::brave::search(&self.connect_brave_key, &query) {
                        Ok(results) => {
                            if !results.is_empty() {
                                prompt_lines.push(
                                    "All temperatures must be in Celsius (metric units). Do not use Fahrenheit."
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
                            if results.is_empty() {
                                self.add_system_message("Brave search returned no results");
                            }
                        }
                        Err(error) => {
                            self.add_system_message(&format!(
                                "Brave search error: {}",
                                error
                            ));
                        }
                    }
                } else {
                    self.add_system_message("Brave search enabled but API key is missing");
                }
            }
        }
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

    fn last_user_message_content(&self) -> Option<String> {
        self.chat_history
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.clone())
    }
}
