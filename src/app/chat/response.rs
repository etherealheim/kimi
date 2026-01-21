use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AgentEvent};
use crate::storage::ConversationData;
use chrono::Local;
use color_eyre::Result;

impl App {
    pub fn check_agent_response(&mut self) {
        if let Some(rx) = &self.agent_rx
            && let Ok(event) = rx.try_recv()
        {
            match event {
                AgentEvent::Response(response) => {
                    self.is_loading = false;
                    self.is_searching = false;
                    self.is_analyzing = false;
                    self.last_response = Some(response.clone());
                    let display_name = if self.personality_enabled {
                        self.personality_name.clone()
                    } else {
                        None
                    };
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: response.clone(),
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name,
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

                    let _ = self.queue_realtime_memory_extraction(&response);
                }
                AgentEvent::Error(error) => {
                    self.is_loading = false;
                    self.is_searching = false;
                    self.is_analyzing = false;
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

                        let messages: Vec<crate::storage::ConversationMessage> = self
                            .chat_history
                            .iter()
                            .map(|message| {
                                let role = match message.role {
                                    MessageRole::User => "User",
                                    MessageRole::Assistant => "Assistant",
                                    MessageRole::System => "System",
                                };
                                crate::storage::ConversationMessage {
                                    role: role.to_string(),
                                    content: message.content.clone(),
                                    timestamp: message.timestamp.clone(),
                                    display_name: message.display_name.clone(),
                                }
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
                AgentEvent::MemoryExtracted(payload) => {
                    let trimmed = payload.trim();
                    if !trimmed.is_empty() {
                        let extracted = crate::services::memories::parse_memory_blocks(trimmed);
                        let extracted = crate::services::memories::filter_extracted_blocks(extracted);
                        if !extracted.contexts.is_empty() {
                            match crate::services::memories::read_memories() {
                                Ok(existing) => {
                                    let current =
                                        crate::services::memories::parse_memory_blocks(&existing);
                                    let current_snapshot = current.to_string();
                                    let merged = crate::services::memories::merge_memory_blocks(
                                        current,
                                        extracted,
                                    );
                                    let merged_snapshot = merged.to_string();
                                    if merged_snapshot == current_snapshot {
                                        return;
                                    }
                                    if let Err(error) =
                                        crate::services::memories::write_memories(&merged)
                                    {
                                        self.add_system_message(&format!(
                                            "Memories update error: {}",
                                            error
                                        ));
                                    } else {
                                        self.show_status_toast("MEMORY SAVED");
                                    }
                                }
                                Err(error) => {
                                    self.add_system_message(&format!(
                                        "Memories read error: {}",
                                        error
                                    ));
                                }
                            }
                        }
                    }
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
