use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, Navigable};
use color_eyre::Result;

impl App {
    pub fn open_history(&mut self) {
        self.mode = AppMode::History;
        self.history_selected_index = 0;
        self.history_filter.clear();
        self.history_filter_active = false;
        self.history_delete_all_active = false;
        self.load_history_list();
        // Stop TTS when opening history
        if let Some(tts) = &self.tts_service {
            tts.stop();
        }
    }

    pub fn close_history(&mut self) {
        self.mode = AppMode::Chat;
        self.chat_history.clear();
        self.chat_input.clear();
        self.current_conversation_id = None;
        self.personality_text = None;
        if let Some(agent) = &self.current_agent {
            let agent_name = agent.name.clone();
            let _ = self.load_agent(&agent_name);
        }
        self.history_delete_all_active = false;
    }

    pub(crate) fn load_history_list(&mut self) {
        if let Some(storage) = &self.storage {
            self.history_conversations = if self.history_filter.is_empty() {
                storage.load_conversations().unwrap_or_default()
            } else {
                storage
                    .filter_conversations(self.history_filter.content())
                    .unwrap_or_default()
            };
        }
        if self.history_selected_index >= self.history_conversations.len() {
            self.history_selected_index = self.history_conversations.len().saturating_sub(1);
        }
    }

    pub fn select_history_conversation(&mut self, conversation_id: i64) {
        if let Some(index) = self
            .history_conversations
            .iter()
            .position(|conv| conv.id == conversation_id)
        {
            self.history_selected_index = index;
        }
    }

    pub fn load_history_conversation(&mut self) -> Result<()> {
        let conv = self
            .history_conversations
            .get(self.history_selected_index)
            .ok_or_else(|| color_eyre::eyre::eyre!("Invalid conversation selection"))?;
        let conv_id = conv.id;
        let agent_name = conv.agent_name.clone();

        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Storage not initialized"))?;
        let (_agent_name, messages) = storage.load_conversation(conv_id)?;

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
        let conv_id = conv.id;
        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Storage not initialized"))?;
        storage.delete_conversation(conv_id)?;
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
        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Storage not initialized"))?;
        storage.delete_all_conversations()?;
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
    }

    pub fn previous_history_item(&mut self) {
        HistoryNavigable::new(self).previous_item();
    }
}
