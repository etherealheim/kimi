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
