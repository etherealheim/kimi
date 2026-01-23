use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::MessageRole;
use crate::app::{AgentEvent, App};
use crate::storage::ConversationMessage;
use color_eyre::Result;

pub(crate) const PENDING_SUMMARY_LABEL: &str = "Generating";

impl App {
    pub(crate) fn parse_summary_pair(summary: &str) -> (String, String) {
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
        words
            .get(..max_words)
            .map_or_else(|| summary.to_string(), |slice| slice.join(" "))
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

    pub(crate) fn build_conversation_messages(&self) -> Vec<ConversationMessage> {
        self.chat_history
            .iter()
            .map(|message| {
                let role = match message.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                };
                ConversationMessage {
                    role: role.to_string(),
                    content: message.content.clone(),
                    timestamp: message.timestamp.clone(),
                    display_name: message.display_name.clone(),
                }
            })
            .collect()
    }

    fn save_pending_conversation(&mut self, messages: &[ConversationMessage]) -> Result<()> {
        if !self.ensure_storage() {
            return Err(color_eyre::eyre::eyre!("Storage not initialized"));
        }
        let Some(storage) = &self.storage else {
            return Err(color_eyre::eyre::eyre!("Storage not initialized"));
        };
        let agent_name = self
            .current_agent
            .as_ref()
            .map_or("unknown", |agent| agent.name.as_str());
        
        let runtime = self
            .storage_runtime()
            .ok_or_else(|| color_eyre::eyre::eyre!("Storage runtime not initialized"))?;
        if let Some(conversation_id) = &self.current_conversation_id {
            let conv_id_clone = conversation_id.clone();
            runtime.block_on(async {
                storage.update_conversation(
                    &conv_id_clone,
                    PENDING_SUMMARY_LABEL,
                    PENDING_SUMMARY_LABEL,
                    messages,
                ).await
            })?;
        } else {
            let data = crate::storage::ConversationData::new(agent_name, messages)
                .with_summary(PENDING_SUMMARY_LABEL)
                .with_detailed_summary(PENDING_SUMMARY_LABEL);
            let conversation_id = runtime.block_on(async {
                storage.save_conversation(data).await
            })?;
            self.current_conversation_id = Some(conversation_id);
        }
        Ok(())
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

        // Check if conversation was modified since loading
        let current_non_system_count = self.chat_history.iter()
            .filter(|msg| msg.role != MessageRole::System)
            .count();
        
        let conversation_modified = self.loaded_conversation_message_count != Some(current_non_system_count);

        // Only generate summary if conversation was actually modified
        if conversation_modified {
            self.is_generating_summary = true;
            self.summary_active = true;

            let context = self.build_summary_context();
            let messages = self.build_conversation_messages();
            if let Err(error) = self.save_pending_conversation(&messages) {
                self.show_status_toast(format!("HISTORY SAVE FAILED: {}", error));
            }
            let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;

            Self::spawn_summary_generation_thread(
                agent,
                manager,
                context,
                agent_tx,
            );
        }

        self.open_history();
        Ok(())
    }
}
