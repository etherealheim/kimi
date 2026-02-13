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
        let agent_name = self
            .current_agent
            .as_ref()
            .map_or("unknown", |agent| agent.name.as_str())
            .to_string();

        let (storage, runtime) = self.storage_with_runtime()?;
        if let Some(conversation_id) = &self.current_conversation_id {
            runtime.block_on(storage.update_conversation(
                conversation_id,
                PENDING_SUMMARY_LABEL,
                PENDING_SUMMARY_LABEL,
                messages,
            ))?;
        } else {
            let data = crate::storage::ConversationData::new(&agent_name, messages)
                .with_summary(PENDING_SUMMARY_LABEL)
                .with_detailed_summary(PENDING_SUMMARY_LABEL);
            let conversation_id = runtime.block_on(storage.save_conversation(data))?;
            self.current_conversation_id = Some(conversation_id);
        }
        Ok(())
    }

    /// Spawns a background thread to generate conversation summary.
    /// The thread is fully self-contained: it carries the conversation_id and messages
    /// so the result can be saved without depending on current app state.
    fn spawn_summary_generation_thread(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        context: String,
        conversation_id: String,
        conversation_messages: Vec<crate::storage::ConversationMessage>,
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
            let _ = agent_tx.send(AgentEvent::SummaryGenerated {
                summary: payload,
                conversation_id,
                messages: conversation_messages,
            });
        });
    }

    pub fn exit_chat_to_history(&mut self) -> Result<()> {
        // IMMEDIATELY change to history mode for instant UI feedback
        self.mode = crate::app::AppMode::History;
        self.history_selected_index = 0;
        self.history_filter.clear();
        self.history_filter_active = false;
        self.history_delete_all_active = false;
        
        // Stop TTS immediately
        if let Some(tts) = &self.tts_service {
            tts.stop();
        }
        
        // Now handle chat saving/summary (after mode change)
        if self.chat_history.is_empty() {
            // Load history data after mode change
            let _ = self.ensure_storage();
            self.load_history_list();
            return Ok(());
        }

        // Always generate summary for conversations with messages
        if !self.chat_history.is_empty() {
            let context = self.build_summary_context();
            let messages = self.build_conversation_messages();
            
            // Quick save with pending label (this is relatively fast - local SQLite)
            if let Err(error) = self.save_pending_conversation(&messages) {
                self.show_status_toast(format!("HISTORY SAVE FAILED: {}", error));
            }
            
            // Validate dependencies BEFORE setting flags.
            // If this fails, we skip summary generation but still load history normally.
            if let Ok((agent, manager, agent_tx)) = self.get_agent_chat_dependencies() {
                self.is_generating_summary = true;
                self.summary_active = true;

                // Capture conversation_id now — the thread must be self-contained
                // so it works even if the user opens a new chat before it finishes.
                let conversation_id = self
                    .current_conversation_id
                    .clone()
                    .unwrap_or_default();

                // Summary generation happens in background thread (non-blocking)
                Self::spawn_summary_generation_thread(
                    agent,
                    manager,
                    context,
                    conversation_id,
                    messages.clone(),
                    agent_tx,
                );
            }
        }

        // Load history data (this might take a moment if many conversations)
        let _ = self.ensure_storage();
        self.load_history_list();
        if let Some(conversation_id) = self.current_conversation_id.clone() {
            self.select_history_conversation(&conversation_id);
        }
        
        Ok(())
    }
}
