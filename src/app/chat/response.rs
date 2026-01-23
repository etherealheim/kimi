use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AgentEvent};
use crate::storage::{ConversationData, ConversationMessage};
use chrono::Local;
use color_eyre::Result;

impl App {
    pub fn check_agent_response(&mut self) {
        if let Some(rx) = &self.agent_rx
            && let Ok(event) = rx.try_recv()
        {
            match event {
                AgentEvent::ResponseWithContext {
                    response,
                    context_usage,
                } => {
                    self.is_loading = false;
                    self.is_searching = false;
                    self.is_fetching_notes = false;
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
                        context_usage,
                    });

                    if self.chat_auto_scroll {
                        self.chat_scroll_offset = 0;
                    }

                    if let Err(error) = self.persist_conversation_messages() {
                        self.add_system_message(&format!("HISTORY SAVE FAILED: {}", error));
                    }
                    
                    // Update emotions after each response
                    self.maybe_update_emotions(&response);

                    if self.auto_tts_enabled
                        && let Some(tts) = &self.tts_service
                        && tts.is_configured()
                    {
                        let _ = tts.speak_text(&response);
                    }
                }
                AgentEvent::Error(error) => {
                    self.is_loading = false;
                    self.is_searching = false;
                    self.is_fetching_notes = false;
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!("Error: {}", error),
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name: None,
                        context_usage: None,
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

                    self.ensure_storage();
                    if let (Some(storage), Some(rt)) =
                        (self.storage.as_ref(), self.storage_runtime())
                    {
                        let agent_name = self
                            .current_agent
                            .as_ref()
                            .map_or("unknown", |agent| agent.name.as_str());

                        let messages = self.build_conversation_messages();

                        let (short_summary, detailed_summary) =
                            Self::parse_summary_pair(&summary);
                        self.maybe_spawn_identity_reflection(&detailed_summary);

                        if let Some(conversation_id) = &self.current_conversation_id {
                            let conv_id_clone = conversation_id.clone();
                            let _ = rt.block_on(async {
                                storage.update_conversation(
                                    &conv_id_clone,
                                    &short_summary,
                                    &detailed_summary,
                                    &messages,
                                ).await
                            });
                            
                            // Save messages with embeddings
                            let _ = self.save_messages_with_embeddings(rt, storage, conversation_id, &messages);
                        } else {
                            let conversation_data = ConversationData::new(agent_name, &messages)
                                .with_summary(&short_summary)
                                .with_detailed_summary(&detailed_summary);

                            if let Ok(conversation_id) = rt.block_on(async {
                                storage.save_conversation(conversation_data).await
                            }) {
                                // Save messages with embeddings
                                let _ = self.save_messages_with_embeddings(rt, storage, &conversation_id, &messages);
                                self.current_conversation_id = Some(conversation_id);
                            }
                        }
                    }
                    if self.mode != crate::app::AppMode::History {
                        self.open_history();
                    } else if let Some(conversation_id) = self.current_conversation_id.clone() {
                        self.load_history_list();
                        self.select_history_conversation(&conversation_id);
                    }
                }
                AgentEvent::SystemMessage(message) => {
                    self.is_loading = false;
                    self.is_searching = false;
                    self.is_fetching_notes = false;
                    self.chat_history.push(ChatMessage {
                        role: MessageRole::System,
                        content: message,
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        display_name: None,
                        context_usage: None,
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
                AgentEvent::CacheObsidianNotes { query, notes } => {
                    // Cache notes for follow-up questions
                    self.cached_obsidian_notes = Some((query, notes));
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

    fn persist_conversation_messages(&mut self) -> Result<()> {
        if !self.ensure_storage() {
            return Err(color_eyre::eyre::eyre!("Storage not initialized"));
        }
        let agent_name = self
            .current_agent
            .as_ref()
            .map_or("unknown", |agent| agent.name.as_str());
        let messages = self.build_conversation_messages();

        let new_conversation_id = {
            let Some(storage) = &self.storage else {
                return Err(color_eyre::eyre::eyre!("Storage not initialized"));
            };
            let Some(runtime) = self.storage_runtime() else {
                return Err(color_eyre::eyre::eyre!("Storage runtime not initialized"));
            };

            let mut new_conversation_id: Option<String> = None;
            let conversation_id =
                if let Some(conversation_id) = self.current_conversation_id.clone() {
                    let conversation_id_clone = conversation_id.clone();
                    runtime.block_on(async {
                        storage.update_conversation_messages(&conversation_id_clone, &messages).await
                    })?;
                    conversation_id
                } else {
                    let data = ConversationData::new(agent_name, &messages);
                    let conversation_id = runtime.block_on(async {
                        storage.save_conversation(data).await
                    })?;
                    new_conversation_id = Some(conversation_id.clone());
                    conversation_id
                };

            let _ = self.save_messages_with_embeddings(
                runtime,
                storage,
                &conversation_id,
                &messages,
            );
            new_conversation_id
        };

        if let Some(conversation_id) = new_conversation_id {
            self.current_conversation_id = Some(conversation_id);
        }
        Ok(())
    }

    fn save_messages_with_embeddings(
        &self,
        runtime: &tokio::runtime::Runtime,
        storage: &crate::storage::StorageManager,
        conversation_id: &str,
        messages: &[ConversationMessage],
    ) -> Result<()> {
        runtime.block_on(async {
            for message in messages {
                let embedding = crate::services::retrieval::generate_message_embedding(&message.content)
                    .await
                    .ok()
                    .flatten();
                let update = crate::storage::MessageEmbeddingUpdate {
                    conversation_id,
                    role: &message.role,
                    content: &message.content,
                    timestamp: &message.timestamp,
                    display_name: message.display_name.as_deref(),
                    embedding,
                };
                let _ = storage.update_message_embedding(update).await;
            }
            Ok(())
        })
    }
}
