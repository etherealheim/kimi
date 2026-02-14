use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, Navigable};
use color_eyre::Result;

impl App {
    pub fn close_history(&mut self) {
        self.mode = AppMode::Chat;
        self.chat_history.clear();
        self.chat_input.clear();
        self.current_conversation_id = None;
        self.personality_text = None;
        self.cached_recall_context = None;
        if let Some(agent) = &self.current_agent {
            let agent_name = agent.name.clone();
            let _ = self.load_agent(&agent_name);
        }
        self.history_delete_all_active = false;

        // Clear summary animation so it doesn't bleed into the new chat.
        // The background thread will still finish and save — we just stop showing the spinner.
        self.summary_active = false;
        self.summary_frame = 0;
        self.last_summary_tick = None;
    }

    pub(crate) fn load_history_list(&mut self) {
        self.ensure_storage();
        let Some(storage) = &self.storage else {
            return;
        };
        let Some(runtime) = self.storage_runtime() else {
            return;
        };

        let limit = self.history_page_size;
        self.history_conversations = if self.history_filter.is_empty() {
            let loaded = runtime
                .block_on(async {
                    storage.load_conversations_with_limit(limit + 1).await.ok()
                })
                .unwrap_or_default();

            // Check if there are more by requesting limit+1
            self.history_has_more = loaded.len() > limit;

            // Return only the requested limit
            loaded.into_iter().take(limit).collect()
        } else {
            runtime
                .block_on(async {
                    storage.filter_conversations(self.history_filter.content()).await.ok()
                })
                .unwrap_or_default()
        };

        if self.history_selected_index >= self.history_conversations.len() {
            self.history_selected_index = self.history_conversations.len().saturating_sub(1);
        }
    }

    pub fn load_more_history(&mut self) {
        if !self.history_has_more || !self.history_filter.is_empty() {
            return;
        }

        self.ensure_storage();
        let Some(storage) = &self.storage else {
            return;
        };
        let Some(runtime) = self.storage_runtime() else {
            return;
        };

        let current_count = self.history_conversations.len();
        let new_limit = current_count + self.history_page_size;

        let loaded = runtime
            .block_on(async {
                storage.load_conversations_with_limit(new_limit + 1).await.ok()
            })
            .unwrap_or_default();

        // Check if there are more
        self.history_has_more = loaded.len() > new_limit;

        // Update conversations
        self.history_conversations = loaded.into_iter().take(new_limit).collect();
    }

    pub fn select_history_conversation(&mut self, conversation_id: &str) {
        if let Some(index) = self
            .history_conversations
            .iter()
            .position(|conv| conv.id == conversation_id)
        {
            self.history_selected_index = index;
            return;
        }
        let normalized = normalize_conversation_id(conversation_id);
        if let Some(index) = self
            .history_conversations
            .iter()
            .position(|conv| normalize_conversation_id(&conv.id) == normalized)
        {
            self.history_selected_index = index;
        }
    }

    pub fn load_history_conversation(&mut self) -> Result<()> {
        let conv = self
            .history_conversations
            .get(self.history_selected_index)
            .ok_or_else(|| color_eyre::eyre::eyre!("Invalid conversation selection"))?;
        let conv_id = conv.id.clone();
        let agent_name = conv.agent_name.clone();

        let (storage, runtime) = self.storage_with_runtime()?;
        let (_agent_name, messages) = runtime.block_on(storage.load_conversation(&conv_id))?;

        self.load_agent(&agent_name)?;

        self.chat_history.clear();
        for msg in messages {
            let role = match msg.role.as_str() {
                "User" => MessageRole::User,
                "Assistant" => MessageRole::Assistant,
                _ => MessageRole::System,
            };
            self.chat_history.push(ChatMessage {
                role,
                content: msg.content,
                timestamp: msg.timestamp,
                display_name: msg.display_name,
                context_usage: None,
            });
        }

        self.current_conversation_id = Some(conv_id);
        self.chat_scroll_offset = 0;
        self.mode = AppMode::Chat;

        if let Some(tts) = &self.tts_service {
            tts.stop();
        }
        Ok(())
    }

    pub fn delete_history_conversation(&mut self) -> Result<()> {
        let conv = self
            .history_conversations
            .get(self.history_selected_index)
            .ok_or_else(|| color_eyre::eyre::eyre!("Invalid conversation selection"))?;
        let conv_id = conv.id.clone();
        let (storage, runtime) = self.storage_with_runtime()?;
        runtime.block_on(storage.delete_conversation(&conv_id))?;
        
        self.load_history_list();
        if self.history_selected_index >= self.history_conversations.len()
            && self.history_selected_index > 0
        {
            self.history_selected_index -= 1;
        }
        Ok(())
    }

    pub fn open_history_delete_all(&mut self) {
        self.history_delete_all_active = true;
        self.history_delete_all_confirm_delete = false;
    }

    pub fn cancel_history_delete_all(&mut self) {
        self.history_delete_all_active = false;
        self.history_delete_all_confirm_delete = false;
    }

    pub fn toggle_history_delete_all_choice(&mut self) {
        self.history_delete_all_confirm_delete = !self.history_delete_all_confirm_delete;
    }

    pub fn confirm_history_delete_all(&mut self) -> Result<()> {
        if !self.history_delete_all_confirm_delete {
            self.cancel_history_delete_all();
            return Ok(());
        }
        let (storage, runtime) = self.storage_with_runtime()?;
        runtime.block_on(storage.delete_all_conversations())?;
        
        self.history_conversations.clear();
        self.history_selected_index = 0;
        self.history_delete_all_active = false;
        self.show_status_toast("HISTORY CLEARED");
        Ok(())
    }

    pub fn toggle_history_filter(&mut self) {
        self.history_filter_active = !self.history_filter_active;
        if !self.history_filter_active {
            self.history_filter.clear();
            self.load_history_list();
        }
    }

    pub fn add_history_filter_char(&mut self, character: char) {
        self.history_filter.add_char(character);
        self.load_history_list();
    }

    pub fn remove_history_filter_char(&mut self) {
        self.history_filter.remove_char();
        self.load_history_list();
    }
}

fn normalize_conversation_id(value: &str) -> &str {
    value.strip_prefix("conversation:").unwrap_or(value)
}

// Navigation for history items
pub struct HistoryNavigable<'a> {
    app: &'a mut App,
}

impl<'a> HistoryNavigable<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }
}

impl<'a> Navigable for HistoryNavigable<'a> {
    fn get_item_count(&self) -> usize {
        self.app.history_conversations.len()
    }

    fn get_selected_index(&self) -> usize {
        self.app.history_selected_index
    }

    fn set_selected_index(&mut self, index: usize) {
        self.app.history_selected_index = index;
    }
}

// Convenience methods for history navigation
impl App {
    pub fn next_history_item(&mut self) {
        HistoryNavigable::new(self).next_item();

        // Auto-load more when approaching the end
        if self.history_has_more {
            let near_end = self.history_selected_index >= self.history_conversations.len().saturating_sub(5);
            if near_end {
                self.load_more_history();
            }
        }
    }

    pub fn previous_history_item(&mut self) {
        HistoryNavigable::new(self).previous_item();
    }
}
