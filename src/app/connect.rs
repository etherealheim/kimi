use crate::app::types::{ChatMessage, MessageRole};
use crate::app::{App, AppMode, Navigable};
use crate::config::Config;
use crate::services::TTSService;
use chrono::Local;
use color_eyre::Result;
use std::path::Path;

impl App {
    pub fn open_connect(&mut self) {
        // Stay in CommandMenu mode but show provider selection
        self.mode = AppMode::Connect;
        self.connect_selected_provider = 0;
        self.connect_api_key_input.clear();
        self.connect_current_provider = None;

        // Load existing keys from config
        if let Ok(config) = Config::load() {
            self.connect_elevenlabs_key = config.elevenlabs.api_key.clone();
            self.connect_venice_key = config.venice.api_key.clone();
            self.connect_brave_key = config.brave.api_key.clone();
            self.connect_obsidian_vault = config.obsidian.vault_path.clone();
        }
    }

    pub fn select_connect_provider(&mut self) {
        if let Some(provider) = self.connect_providers.get(self.connect_selected_provider) {
            self.connect_current_provider = Some(provider.clone());

            // Load existing key for this provider
            match provider.as_str() {
                "ElevenLabs" => {
                    self.connect_api_key_input
                        .set_content(self.connect_elevenlabs_key.clone());
                }
                "Venice AI" => {
                    self.connect_api_key_input
                        .set_content(self.connect_venice_key.clone());
                }
                "Brave Search" => {
                    self.connect_api_key_input
                        .set_content(self.connect_brave_key.clone());
                }
                "Obsidian" => {
                    self.connect_api_key_input
                        .set_content(self.connect_obsidian_vault.clone());
                }
                _ => {}
            }

            // Switch to API key input mode
            self.mode = AppMode::ApiKeyInput;
        }
    }

    pub fn close_api_key_input(&mut self) {
        // Go back to provider selection
        self.mode = AppMode::Connect;
        self.connect_api_key_input.clear();
        self.connect_current_provider = None;
    }

    pub fn save_api_key(&mut self) -> Result<()> {
        if let Some(provider) = self.connect_current_provider.clone() {
            let provider_name = provider.clone();
            let mut did_save = false;
            match provider.as_str() {
                "ElevenLabs" => {
                    self.connect_elevenlabs_key = self.connect_api_key_input.content().to_string();
                    if let Ok(mut config) = Config::load() {
                        config.elevenlabs.api_key = self.connect_elevenlabs_key.clone();
                        let _ = config.save();
                    }
                    did_save = true;

                    // Update TTS service if configured
                    if let Some(tts) = &mut self.tts_service {
                        *tts = TTSService::new(
                            self.connect_elevenlabs_key.clone(),
                            "default".to_string(),
                            "eleven_monolingual_v1".to_string(),
                        );
                    }
                }
                "Venice AI" => {
                    let candidate_key = self.connect_api_key_input.content().to_string();
                    if crate::services::venice::fetch_text_models(&candidate_key).is_ok() {
                        self.connect_venice_key = candidate_key;
                        if let Ok(mut config) = Config::load() {
                            config.venice.api_key = self.connect_venice_key.clone();
                            let _ = config.save();
                        }
                        let _ = self.refresh_available_models();
                        if let Some(manager) = &mut self.agent_manager {
                            manager.set_venice_api_key(self.connect_venice_key.clone());
                        }
                        did_save = true;
                    } else {
                        self.chat_history.push(ChatMessage {
                            role: MessageRole::System,
                            content: "Venice API key invalid or models unavailable".to_string(),
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                            display_name: None,
                            context_usage: None,
                        });
                    }
                }
                "Brave Search" => {
                    self.connect_brave_key = self.connect_api_key_input.content().to_string();
                    if let Ok(mut config) = Config::load() {
                        config.brave.api_key = self.connect_brave_key.clone();
                        let _ = config.save();
                    }
                    did_save = true;
                }
                "Obsidian" => {
                    let candidate_path = self.connect_api_key_input.content().to_string();
                    if candidate_path.trim().is_empty() {
                        self.chat_history.push(ChatMessage {
                            role: MessageRole::System,
                            content: "Obsidian vault path cannot be empty".to_string(),
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                            display_name: None,
                            context_usage: None,
                        });
                    } else if !Path::new(&candidate_path).is_dir() {
                        self.chat_history.push(ChatMessage {
                            role: MessageRole::System,
                            content: "Obsidian vault path is not a directory".to_string(),
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                            display_name: None,
                            context_usage: None,
                        });
                    } else {
                        self.connect_obsidian_vault = candidate_path;
                        if let Ok(mut config) = Config::load() {
                            config.obsidian.vault_path = self.connect_obsidian_vault.clone();
                            let _ = config.save();
                        }
                        did_save = true;
                    }
                }
                _ => {}
            }

            if did_save {
                let _ = provider_name;
                self.show_status_toast("KEY SAVED");
            }
        }

        // Go back to chat (exit connect flow)
        self.mode = AppMode::Chat;
        self.connect_api_key_input.clear();
        self.connect_current_provider = None;
        Ok(())
    }

    pub fn close_connect(&mut self) {
        self.mode = AppMode::Chat;
        self.connect_selected_provider = 0;
        self.connect_api_key_input.clear();
        self.connect_current_provider = None;
    }

    pub fn add_api_key_char(&mut self, character: char) {
        self.connect_api_key_input.add_char(character);
    }

    pub fn remove_api_key_char(&mut self) {
        self.connect_api_key_input.remove_char();
    }
}

// Navigation for provider selection
pub struct ConnectProviderNavigable<'a> {
    app: &'a mut App,
}

impl<'a> ConnectProviderNavigable<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }
}

impl<'a> Navigable for ConnectProviderNavigable<'a> {
    fn get_item_count(&self) -> usize {
        self.app.connect_providers.len()
    }

    fn get_selected_index(&self) -> usize {
        self.app.connect_selected_provider
    }

    fn set_selected_index(&mut self, index: usize) {
        self.app.connect_selected_provider = index;
    }
}

// Convenience methods for connect provider navigation
impl App {
    pub fn next_connect_provider(&mut self) {
        ConnectProviderNavigable::new(self).next_item();
    }

    pub fn previous_connect_provider(&mut self) {
        ConnectProviderNavigable::new(self).previous_item();
    }
}
