mod chat;
pub(crate) use chat::PENDING_SUMMARY_LABEL;
mod command;
mod connect;
mod help;
mod history;
mod models;
mod navigation;
mod identity;
mod personality;
mod scroll;
#[path = "text-input.rs"]
mod text_input;
mod types;

pub use command::cmd_quit;
pub use navigation::Navigable;
pub use text_input::TextInput;
pub use types::*;

use crate::agents::{Agent, AgentManager};
use crate::config::Config;
use crate::services::TTSService;
use crate::services::clipboard::ClipboardService;
use crate::storage::{ConversationSummary, StorageManager};
use color_eyre::Result;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// Application mode state
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    CommandMenu,
    Chat,
    ModelSelection,
    Connect,
    ApiKeyInput,
    History,
    Help,
    PersonalitySelection,
    PersonalityCreate,
    IdentityView,
}

/// Events from the agent processing thread
pub enum AgentEvent {
    ResponseWithContext {
        response: String,
        context_usage: Option<ContextUsage>,
    },
    Error(String),
    SummaryGenerated(String),
    SystemMessage(String),
    DownloadFinished,
    DownloadProgress(u8),
    ConversionFinished,
    CacheObsidianNotes {
        query: String,
        notes: Vec<crate::services::obsidian::NoteSnippet>,
    },
}

/// Main application state
pub struct App {
    pub mode: AppMode,
    pub previous_mode: Option<AppMode>,
    pub should_quit: bool,
    pub input: String,
    pub selected_index: usize,
    pub menu_items: Vec<MenuItem>,
    pub messages: Vec<String>,
    pub command_handlers: HashMap<String, fn() -> Result<String>>,

    // Chat-related fields
    pub chat_history: Vec<ChatMessage>,
    pub chat_history_by_agent: HashMap<String, Vec<ChatMessage>>,
    pub chat_input: TextInput,
    pub chat_attachments: Vec<ChatAttachment>,
    pub next_attachment_id: usize,
    pub current_agent: Option<Agent>,
    pub is_loading: bool,
    pub is_searching: bool,
    pub is_fetching_notes: bool,
    pub last_response: Option<String>,
    pub agent_manager: Option<AgentManager>,
    pub tts_service: Option<TTSService>,
    pub agent_rx: Option<Receiver<AgentEvent>>,
    pub agent_tx: Option<Sender<AgentEvent>>,
    pub auto_tts_enabled: bool,
    pub chat_scroll_offset: usize,
    pub chat_auto_scroll: bool, // Whether to auto-scroll to bottom on new messages
    pub cached_obsidian_notes: Option<(String, Vec<crate::services::obsidian::NoteSnippet>)>, // (query, notes) for follow-up questions

    // Model selection fields
    pub available_models: HashMap<String, Vec<AvailableModel>>,
    pub selected_models: HashMap<String, Vec<String>>,
    pub model_selection_index: usize,
    pub model_selection_items: Vec<ModelSelectionItem>,

    // Connect fields
    pub connect_elevenlabs_key: String,
    pub connect_venice_key: String,
    pub connect_gab_key: String,
    pub connect_brave_key: String,
    pub connect_obsidian_vault: String,
    pub connect_providers: Vec<String>,
    pub connect_selected_provider: usize,
    pub connect_api_key_input: TextInput,
    pub connect_current_provider: Option<String>,
    // Personality fields
    pub personality_items: Vec<String>,
    pub personality_selected_index: usize,
    pub personality_create_input: TextInput,
    pub personality_name: Option<String>,

    // History fields
    pub history_conversations: Vec<ConversationSummary>,
    pub history_selected_index: usize,
    pub history_filter: TextInput,
    pub history_filter_active: bool,
    pub history_delete_all_active: bool,
    pub history_delete_all_confirm_delete: bool,
    pub storage: Option<StorageManager>,
    pub storage_runtime: Option<tokio::runtime::Runtime>,
    pub is_generating_summary: bool,
    pub current_conversation_id: Option<String>,
    pub loaded_conversation_message_count: Option<usize>,
    pub status_toast: Option<StatusToast>,
    pub clipboard_service: ClipboardService,
    pub personality_enabled: bool,
    pub personality_enabled_by_agent: HashMap<String, bool>,
    pub personality_text: Option<String>,
    pub loading_frame: u8,
    pub last_loading_tick: Option<std::time::Instant>,
    pub download_active: bool,
    pub download_frame: u8,
    pub last_download_tick: Option<std::time::Instant>,
    pub download_progress: Option<u8>,
    pub conversion_active: bool,
    pub conversion_frame: u8,
    pub last_conversion_tick: Option<std::time::Instant>,
    pub summary_active: bool,
    pub summary_frame: u8,
    pub last_summary_tick: Option<std::time::Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn menu_item(name: &str, description: &str) -> MenuItem {
    MenuItem {
        name: name.to_string(),
        description: description.to_string(),
    }
}

fn parse_model_command(command: &str) -> Option<(String, String)> {
    let (agent_name, model_name) = command.split_once(':')?;
    if !matches!(agent_name, "translate" | "chat") {
        return None;
    }
    Some((agent_name.to_string(), model_name.to_string()))
}

fn base_menu_items() -> Vec<MenuItem> {
    vec![
        menu_item("models", "Select models per agent"),
        menu_item("connect", "API token configuration"),
        menu_item("personality", "Manage personalities"),
        menu_item("help", "Show keyboard shortcuts"),
        menu_item("quit", "Exit the application"),
    ]
}

impl App {
    /// Creates a new application instance with default settings
    pub fn new() -> Self {
        let available_models: HashMap<String, Vec<AvailableModel>> = HashMap::new();
        let selected_models = [
            ("embeddings", vec!["bge-m3"]),
            ("translate", vec!["translategemma:latest"]),
            ("chat", vec!["gemma3:12b"]),
            ("routing", vec!["functiongemma"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
        .collect();

        let menu_items = base_menu_items();

        let command_handlers = [("quit", cmd_quit as fn() -> Result<String>)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        Self {
            mode: AppMode::Chat, // Start directly in chat mode
            previous_mode: None,
            should_quit: false,
            input: String::new(),
            selected_index: 0,
            menu_items,
            messages: Vec::new(), // Remove welcome message
            command_handlers,
            chat_history: Vec::new(),
            chat_history_by_agent: HashMap::new(),
            chat_input: TextInput::new(),
            chat_attachments: Vec::new(),
            next_attachment_id: 1,
            current_agent: None, // Will be set in init_services
            is_loading: false,
            is_searching: false,
            is_fetching_notes: false,
            last_response: None,
            agent_manager: None,
            tts_service: None,
            agent_rx: None,
            agent_tx: None,
            auto_tts_enabled: false,
            chat_scroll_offset: 0,
            chat_auto_scroll: true, // Start with auto-scroll enabled
            available_models,
            selected_models,
            model_selection_index: 0,
            model_selection_items: Vec::new(),
            connect_elevenlabs_key: String::new(),
            connect_venice_key: String::new(),
            connect_gab_key: String::new(),
            connect_brave_key: String::new(),
            connect_obsidian_vault: String::new(),
            connect_providers: vec![
                "ElevenLabs".to_string(),
                "Venice AI".to_string(),
                "Gab AI".to_string(),
                "Brave Search".to_string(),
                "Obsidian".to_string(),
            ],
            connect_selected_provider: 0,
            connect_api_key_input: TextInput::new(),
            connect_current_provider: None,
            personality_items: Vec::new(),
            personality_selected_index: 0,
            personality_create_input: TextInput::new(),
            personality_name: None,
            history_conversations: Vec::new(),
            history_selected_index: 0,
            history_filter: TextInput::new(),
            history_filter_active: false,
            history_delete_all_active: false,
            history_delete_all_confirm_delete: false,
            storage: None,
            storage_runtime: None,
            is_generating_summary: false,
            current_conversation_id: None,
            loaded_conversation_message_count: None,
            status_toast: None,
            clipboard_service: ClipboardService::new(),
            personality_enabled: false,
            personality_enabled_by_agent: HashMap::new(),
            personality_text: None,
            loading_frame: 0,
            last_loading_tick: None,
            download_active: false,
            download_frame: 0,
            last_download_tick: None,
            download_progress: None,
            conversion_active: false,
            conversion_frame: 0,
            last_conversion_tick: None,
            summary_active: false,
            summary_frame: 0,
            last_summary_tick: None,
            cached_obsidian_notes: None,
        }
    }

    /// Initializes services (agent manager, TTS, storage) with configuration
    pub fn init_services(&mut self, config: &Config) {
        let mut agent_config = config.clone();
        if let Ok(base_personality) = crate::services::personality::read_base_personality() {
            let trimmed = base_personality.trim();
            if !trimmed.is_empty()
                && let Some(chat_config) = agent_config.agents.get_mut("chat")
            {
                chat_config.system_prompt = trimmed.to_string();
            }
        }
        self.agent_manager = Some(AgentManager::new(&agent_config));
        self.connect_venice_key = config.venice.api_key.clone();
        self.connect_gab_key = config.gab.api_key.clone();
        self.connect_brave_key = config.brave.api_key.clone();
        self.connect_obsidian_vault = config.obsidian.vault_path.clone();
        if let Some(manager) = &mut self.agent_manager {
            if !self.connect_venice_key.is_empty() {
                manager.set_venice_api_key(self.connect_venice_key.clone());
            }
            if !self.connect_gab_key.is_empty() {
                manager.set_gab_api_key(self.connect_gab_key.clone());
            }
        }
        self.tts_service = Some(TTSService::new(
            config.elevenlabs.api_key.clone(),
            config.elevenlabs.voice_id.clone(),
            config.elevenlabs.model.clone(),
        ));
        
        let _ = self.ensure_storage();

        let (tx, rx) = channel();
        self.agent_tx = Some(tx);
        self.agent_rx = Some(rx);

        let _ = self.refresh_available_models();
        self.load_selected_models_from_config(config);

        let _ = self.load_agent("chat");
        if !config.personality.selected.is_empty() {
            self.personality_name = Some(config.personality.selected.clone());
        }
    }

    pub fn execute_command(&mut self, command: &str) -> Result<()> {
        // Clear menu input when executing any command
        self.input.clear();
        self.selected_index = 0;

        if let Some((agent_name, model_name)) = parse_model_command(command) {
            self.set_selected_model(&agent_name, &model_name)?;
            self.close_menu();
            return Ok(());
        }

        if command == "personality" {
            self.open_personality_menu()?;
            return Ok(());
        }

        // Check if it's an agent command
        if self.is_agent_command(command) {
            self.load_agent(command)?;
            return Ok(());
        }

        // Check if it's the models command
        if command == "models" {
            self.open_model_selection();
            return Ok(());
        }

        // Check if it's the connect command
        if command == "connect" {
            self.open_connect();
            return Ok(());
        }

        if command == "help" {
            self.open_help();
            return Ok(());
        }

        if let Some(handler) = self.command_handlers.get(command) {
            let result = handler()?;
            if command == "quit" {
                self.should_quit = true;
            } else {
                self.messages.push(result);
            }
        } else {
            self.messages.push(format!("Unknown command: {}", command));
        }
        Ok(())
    }

    pub fn execute_selected(&mut self) -> Result<()> {
        let filtered = self.filtered_items();
        if let Some(item) = filtered.get(self.selected_index) {
            let command = item.name.clone();
            // Execute command and handle errors
            self.execute_command(&command)?;
        }
        Ok(())
    }

    pub(crate) fn ensure_storage_runtime(&mut self) -> bool {
        if self.storage_runtime.is_some() {
            return true;
        }
        self.storage_runtime = tokio::runtime::Runtime::new().ok();
        self.storage_runtime.is_some()
    }

    pub(crate) fn storage_runtime(&self) -> Option<&tokio::runtime::Runtime> {
        self.storage_runtime.as_ref()
    }

    pub(crate) fn ensure_storage(&mut self) -> bool {
        if self.storage.is_some() {
            return true;
        }
        if !self.ensure_storage_runtime() {
            return false;
        }
        let Some(runtime) = self.storage_runtime() else {
            return false;
        };
        self.storage = runtime.block_on(async {
            StorageManager::new().await.ok()
        });
        self.storage.is_some()
    }

    fn rebuild_menu_items(&mut self) {
        self.menu_items = base_menu_items();
    }

    pub fn show_status_toast(&mut self, message: impl Into<String>) {
        self.status_toast = Some(StatusToast::new(message));
    }

    pub fn clear_expired_status_toast(&mut self) {
        let should_clear = self
            .status_toast
            .as_ref()
            .is_some_and(|toast| toast.is_expired(Duration::from_secs(3)));
        if should_clear {
            self.status_toast = None;
        }
    }

    #[must_use]
    pub fn status_toast_message(&self) -> Option<&str> {
        self.status_toast.as_ref().map(|toast| toast.message.as_str())
    }

    pub fn last_assistant_message(&self) -> Option<&str> {
        self.chat_history
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant)
            .map(|message| message.content.as_str())
    }

    pub fn toggle_personality(&mut self) {
        self.personality_enabled = !self.personality_enabled;
        if self.personality_enabled {
            let selected_name = self
                .personality_name
                .clone()
                .unwrap_or_else(crate::services::personality::default_personality_name);
            match crate::services::personality::read_personality(&selected_name) {
                Ok(text) => {
                    self.personality_text = Some(text);
                }
                Err(error) => {
                    self.personality_enabled = false;
                    self.personality_text = None;
                    self.add_system_message(&format!("Personality error: {}", error));
                }
            }
        } else {
            self.personality_text = None;
        }
    }


    fn load_selected_models_from_config(&mut self, config: &Config) {
        for (agent_name, agent_config) in &config.agents {
            let selected = self
                .selected_models
                .entry(agent_name.clone())
                .or_default();
            selected.clear();
            selected.push(agent_config.model.clone());
        }
    }
}

// Include implementations from feature modules
// Methods are added via impl App blocks in each module
