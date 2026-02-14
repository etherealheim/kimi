use crate::app::types::ChatMessage;
use crate::app::{App, AgentEvent};
use crate::storage::{ConversationData, ConversationMessage};
use color_eyre::Result;

impl App {
    pub fn check_agent_response(&mut self) {
        // Drain all pending events to avoid stale status updates lagging behind.
        // Collect first to release the immutable borrow on self before processing.
        let events: Vec<AgentEvent> = self
            .agent_rx
            .as_ref()
            .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
            .unwrap_or_default();
        for event in events {
            match event {
                AgentEvent::ResponseWithContext { response, context_usage } => {
                    self.handle_agent_response(response, context_usage);
                }
                AgentEvent::Error(error) => self.handle_agent_error(error),
                AgentEvent::SummaryGenerated { summary, conversation_id, messages } => {
                    self.handle_summary_generated(summary, conversation_id, messages);
                }
                AgentEvent::SystemMessage(message) => self.handle_system_message(message),
                AgentEvent::StatusUpdate(status) => self.current_activity = Some(status),
                AgentEvent::DownloadFinished { url } => {
                    self.active_downloads.retain(|item| item.url != url);
                }
                AgentEvent::DownloadProgress { url, progress } => {
                    if let Some(item) = self.active_downloads.iter_mut().find(|item| item.url == url) {
                        item.progress = Some(progress);
                    }
                }
                AgentEvent::ConversionFinished => {
                    self.conversion_active = false;
                    self.conversion_frame = 0;
                    self.last_conversion_tick = None;
                }
                AgentEvent::CacheObsidianNotes { query, notes } => {
                    self.cached_obsidian_notes = Some((query, notes));
                }
                AgentEvent::CacheRecallContext { context } => {
                    self.cached_recall_context = Some(context);
                }
                AgentEvent::FollowUpSuggestions { suggestions } => {
                    self.follow_up_suggestions = suggestions;
                    self.suggestion_selected_index = 0;
                    self.suggestion_mode_active = false;
                }
                AgentEvent::TopicsExtracted { topics, conversation_id } => {
                    self.handle_topics_extracted(topics, conversation_id);
                }
                AgentEvent::ProjectEntriesExtracted { results } => {
                    self.handle_project_entries_extracted(results);
                }
            }
        }
    }

    /// Clears all loading/activity flags at once
    fn clear_loading_state(&mut self) {
        self.is_loading = false;
        self.is_searching = false;
        self.is_fetching_notes = false;
        self.current_activity = None;
    }

    fn handle_agent_response(
        &mut self,
        response: String,
        context_usage: Option<crate::app::types::ContextUsage>,
    ) {
        self.clear_loading_state();
        self.last_response = Some(response.clone());

        let display_name = if self.personality_enabled {
            self.personality_name.clone()
        } else {
            None
        };
        self.chat_history
            .push(ChatMessage::assistant(response.clone(), display_name, context_usage));

        if self.chat_auto_scroll {
            self.chat_scroll_offset = 0;
        }

        if let Err(error) = self.persist_conversation_messages() {
            self.add_system_message(&format!("HISTORY SAVE FAILED: {}", error));
        }

        self.maybe_update_emotions(&response);
        self.spawn_follow_up_suggestions(&response);

        if self.auto_tts_enabled
            && let Some(tts) = &self.tts_service
            && tts.is_configured()
        {
            let _ = tts.speak_text(&response);
        }
    }

    fn handle_agent_error(&mut self, error: String) {
        self.clear_loading_state();
        self.chat_history
            .push(ChatMessage::system(format!("Error: {}", error)));

        if self.chat_auto_scroll {
            self.chat_scroll_offset = 0;
        }
    }

    /// Handles a completed summary using only the data carried by the event.
    /// This never forces a mode change — if the user already started a new chat,
    /// the summary is saved silently in the background.
    fn handle_summary_generated(
        &mut self,
        summary: String,
        conversation_id: String,
        messages: Vec<crate::storage::ConversationMessage>,
    ) {
        self.is_generating_summary = false;
        self.summary_active = false;
        self.summary_frame = 0;
        self.last_summary_tick = None;

        let (short_summary, detailed_summary) = Self::parse_summary_pair(&summary);
        self.maybe_spawn_identity_reflection(&detailed_summary);

        // Save summary to storage using the captured conversation_id,
        // not the current one (which may belong to a different chat now).
        self.ensure_storage();
        if let (Some(storage), Some(rt)) = (self.storage.as_ref(), self.storage_runtime()) {
            let _ = rt.block_on(async {
                storage
                    .update_conversation(
                        &conversation_id,
                        &short_summary,
                        &detailed_summary,
                        &messages,
                    )
                    .await
            });

            Self::spawn_background_embeddings(storage.clone(), conversation_id.clone(), messages.clone());
        }

        // Spawn topic extraction in background
        self.maybe_spawn_topic_extraction(&messages, &conversation_id);

        // Only refresh history UI if user is currently viewing it
        if self.mode == crate::app::AppMode::History {
            self.load_history_list();
            self.select_history_conversation(&conversation_id);
        }
    }

    fn handle_system_message(&mut self, message: String) {
        self.clear_loading_state();
        self.chat_history.push(ChatMessage::system(message));
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
            .map_or("unknown", |agent| agent.name.as_str())
            .to_string();
        let messages = self.build_conversation_messages();

        let (storage, runtime) = self.storage_with_runtime()?;
        let conversation_id =
            if let Some(conversation_id) = self.current_conversation_id.clone() {
                runtime.block_on(
                    storage.update_conversation_messages(&conversation_id, &messages),
                )?;
                conversation_id
            } else {
                let data = ConversationData::new(&agent_name, &messages);
                let new_id = runtime.block_on(storage.save_conversation(data))?;
                self.current_conversation_id = Some(new_id.clone());
                new_id
            };

        // Generate embeddings in background thread (non-blocking)
        if let Some(storage) = &self.storage {
            Self::spawn_background_embeddings(storage.clone(), conversation_id, messages);
        }
        Ok(())
    }

    // ── Project topic extraction ──────────────────────────────────────────────

    fn maybe_spawn_topic_extraction(
        &self,
        messages: &[ConversationMessage],
        conversation_id: &str,
    ) {
        // Skip if conversation is too short (fewer than 4 messages)
        let non_system_count = messages
            .iter()
            .filter(|message| message.role != "System")
            .count();
        if non_system_count < 4 {
            return;
        }

        let Some(agent_tx) = self.agent_tx.as_ref().cloned() else {
            return;
        };
        let Ok((agent, manager, _)) = self.get_agent_chat_dependencies() else {
            return;
        };

        let vault_path = self.connect_obsidian_vault_path.clone();
        let conversation_id = conversation_id.to_string();

        // Build conversation content for the LLM
        let content: String = messages
            .iter()
            .filter(|message| message.role != "System")
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join(" ");

        std::thread::spawn(move || {
            let topics = crate::services::projects::extract_topics(&content, &agent, &manager);
            if topics.is_empty() {
                return;
            }

            // Check if any topics match existing projects for entry extraction
            let existing_projects =
                crate::services::projects::list_project_names(&vault_path).unwrap_or_default();
            let matching: Vec<String> = existing_projects
                .iter()
                .filter(|name| {
                    let name_lower = name.to_lowercase();
                    topics.iter().any(|topic| {
                        name_lower.contains(topic) || topic.contains(&name_lower)
                    })
                })
                .cloned()
                .collect();

            // If topics match existing projects, also extract entries
            if !matching.is_empty() {
                let results = crate::services::projects::extract_entries_for_projects(
                    &content, &matching, &agent, &manager,
                );
                if !results.is_empty() {
                    let _ = agent_tx.send(AgentEvent::ProjectEntriesExtracted { results });
                }
            }

            let _ = agent_tx.send(AgentEvent::TopicsExtracted {
                topics,
                conversation_id,
            });
        });
    }

    fn handle_topics_extracted(&mut self, topics: Vec<String>, conversation_id: String) {
        // Store topic mentions in DB
        self.ensure_storage();
        if let (Some(storage), Some(rt)) = (self.storage.as_ref(), self.storage_runtime()) {
            let _ = rt.block_on(async {
                storage.record_topic_mentions(&topics, &conversation_id).await
            });

            // Check if any topic crosses the suggestion threshold
            let frequent = rt
                .block_on(async { storage.load_frequent_topics(3).await })
                .unwrap_or_default();

            if !frequent.is_empty() {
                let vault_path = self.connect_obsidian_vault_path.clone();
                let existing =
                    crate::services::projects::list_project_names(&vault_path).unwrap_or_default();
                let existing_lower: Vec<String> =
                    existing.iter().map(|name| name.to_lowercase()).collect();

                for (topic, _count) in &frequent {
                    // Only suggest if there's no existing project with this name
                    if !existing_lower.contains(topic)
                        && !self.pending_project_suggestions.contains(topic)
                    {
                        self.pending_project_suggestions.push(topic.clone());
                    }
                }
            }
        }
    }

    fn handle_project_entries_extracted(
        &mut self,
        results: Vec<crate::services::projects::ProjectExtractionResult>,
    ) {
        let vault_path = self.connect_obsidian_vault_path.clone();
        for result in &results {
            let _ = crate::services::projects::append_project_entries(
                &vault_path,
                &result.project_name,
                &result.entries,
            );
        }
    }

    /// Spawns a background thread to generate follow-up question suggestions
    fn spawn_follow_up_suggestions(&self, response: &str) {
        let Some(manager) = self.agent_manager.clone() else {
            return;
        };
        let Some(agent) = self.current_agent.clone() else {
            return;
        };
        let Some(agent_tx) = self.agent_tx.clone() else {
            return;
        };

        // Build recent context: last user message + assistant response
        let last_user = self
            .chat_history
            .iter()
            .rev()
            .find(|message| message.role == crate::app::types::MessageRole::User)
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let response = response.to_string();

        std::thread::spawn(move || {
            let prompt = format!(
                "Based on this conversation exchange, suggest exactly 2 short follow-up questions \
                 the user might want to ask next. Each should be concise (under 8 words).\n\n\
                 User: {}\nAssistant: {}\n\n\
                 Return ONLY a JSON array of 2 strings, nothing else:\n\
                 [\"question 1\", \"question 2\"]",
                last_user,
                response.chars().take(500).collect::<String>()
            );

            let messages = vec![
                crate::agents::ChatMessage::system(
                    "You suggest follow-up questions. Output only a JSON array of 2 short strings.",
                ),
                crate::agents::ChatMessage::user(prompt),
            ];

            if let Ok(raw) = manager.chat(&agent, &messages)
                && let Some(suggestions) = parse_suggestion_array(&raw)
            {
                let _ = agent_tx.send(AgentEvent::FollowUpSuggestions { suggestions });
            }
        });
    }

    /// Spawns a background thread to generate and save embeddings without blocking the UI
    fn spawn_background_embeddings(
        storage: crate::storage::StorageManager,
        conversation_id: String,
        messages: Vec<ConversationMessage>,
    ) {
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Runtime::new() else {
                return;
            };
            runtime.block_on(async {
                for message in &messages {
                    let embedding = crate::services::retrieval::generate_message_embedding(&message.content)
                        .await
                        .ok()
                        .flatten();
                    let update = crate::storage::MessageEmbeddingUpdate {
                        conversation_id: &conversation_id,
                        role: &message.role,
                        content: &message.content,
                        timestamp: &message.timestamp,
                        display_name: message.display_name.as_deref(),
                        embedding,
                    };
                    let _ = storage.update_message_embedding(update).await;
                }
            });
        });
    }
}

/// Parses a JSON array of strings from LLM output, handling common quirks
fn parse_suggestion_array(raw: &str) -> Option<Vec<String>> {
    // Try to find JSON array in the response
    let trimmed = raw.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        let end = trimmed.rfind(']')?;
        trimmed.get(start..=end)?
    } else {
        return None;
    };

    let parsed: Vec<String> = serde_json::from_str(json_str).ok()?;
    if parsed.is_empty() {
        return None;
    }

    // Take up to 2 suggestions
    Some(parsed.into_iter().take(2).collect())
}
