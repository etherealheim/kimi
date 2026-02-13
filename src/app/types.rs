/// Information about an available AI model
#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub name: String,
    pub source: ModelSource,
    pub is_available: bool,
}

/// Source of an AI model
#[derive(Debug, Clone, PartialEq)]
pub enum ModelSource {
    Ollama,
    VeniceAPI,
    GabAI,
}

/// Item in the model selection UI
#[derive(Debug, Clone)]
pub struct ModelSelectionItem {
    pub agent_name: String,
    pub model_index: usize,
}

/// Menu item for the command palette
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub name: String,
    pub description: String,
}

/// A chat message with role, content, and timestamp
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: String,
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub context_usage: Option<ContextUsage>,
}

impl ChatMessage {
    fn now_timestamp() -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            timestamp: Self::now_timestamp(),
            display_name: None,
            context_usage: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            timestamp: Self::now_timestamp(),
            display_name: None,
            context_usage: None,
        }
    }

    pub fn assistant(
        content: impl Into<String>,
        display_name: Option<String>,
        context_usage: Option<ContextUsage>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Self::now_timestamp(),
            display_name,
            context_usage,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusToast {
    pub message: String,
    pub created_at: std::time::Instant,
}

impl StatusToast {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            created_at: std::time::Instant::now(),
        }
    }

    pub fn is_expired(&self, duration: std::time::Duration) -> bool {
        self.created_at.elapsed() >= duration
    }
}

/// Role of a chat message
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct ContextUsage {
    pub notes_used: usize,
    pub history_used: usize,
    pub memories_used: usize,
}

#[derive(Debug, Clone)]
pub enum ChatAttachment {
    FilePath {
        token: String,
        path: std::path::PathBuf,
    },
    ClipboardImage {
        token: String,
        png_bytes: Vec<u8>,
    },
}

impl ChatAttachment {
    #[must_use]
    pub fn token(&self) -> &str {
        match self {
            ChatAttachment::FilePath { token, .. } => token,
            ChatAttachment::ClipboardImage { token, .. } => token,
        }
    }

}

/// Represents an individual download in progress
#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub url: String,
    pub progress: Option<u8>,
    pub frame: u8,
    pub last_tick: Option<std::time::Instant>,
}
