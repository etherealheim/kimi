pub mod brave;
#[path = "gab-ai.rs"]
pub mod gab_ai;
pub mod ollama;
pub mod venice;

use crate::config::Config;
use crate::app::ModelSource;
use color_eyre::Result;
use ollama::OllamaClient;
use std::collections::HashMap;
use std::sync::Arc;

/// An AI agent with its configuration
#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub model: String,
    pub system_prompt: String,
    pub model_source: ModelSource,
}

/// Manages AI agents and their interaction with the Ollama backend
#[derive(Clone)]
pub struct AgentManager {
    agents: HashMap<String, Agent>,
    ollama_client: Arc<OllamaClient>,
    venice_api_key: Option<String>,
    gab_api_key: Option<String>,
    gab_base_url: String,
}

impl AgentManager {
    /// Creates a new agent manager from configuration
    pub fn new(config: &Config) -> Self {
        let ollama_client = Arc::new(OllamaClient::new(&config.ollama.url));
        let mut agents = HashMap::new();

        // Load agents from config
        for (name, agent_config) in &config.agents {
            agents.insert(
                name.clone(),
                Agent {
                    name: name.clone(),
                    model: agent_config.model.clone(),
                    system_prompt: agent_config.system_prompt.clone(),
                    model_source: ModelSource::Ollama,
                },
            );
        }

        Self {
            agents,
            ollama_client,
            venice_api_key: None,
            gab_api_key: if config.gab.api_key.trim().is_empty() {
                None
            } else {
                Some(config.gab.api_key.clone())
            },
            gab_base_url: config.gab.base_url.clone(),
        }
    }

    /// Gets an agent by name
    #[must_use]
    pub fn get_agent(&self, name: &str) -> Option<&Agent> {
        self.agents.get(name)
    }

    /// Checks if an agent is ready to use (Ollama running, model available)
    pub fn check_agent_ready(&self, agent: &Agent) -> Result<String> {
        use std::time::Instant;
        let start = Instant::now();

        if agent.model_source == ModelSource::VeniceAPI {
            let api_key = self
                .venice_api_key
                .as_ref()
                .ok_or_else(|| color_eyre::eyre::eyre!("Venice API key not configured"))?;
            let _ = api_key;
            return Ok("Venice API ready".to_string());
        }
        if agent.model_source == ModelSource::GabAI {
            let api_key = self
                .gab_api_key
                .as_ref()
                .ok_or_else(|| color_eyre::eyre::eyre!("Gab AI key not configured"))?;
            let _ = api_key;
            return Ok("Gab AI ready".to_string());
        }

        // Check if Ollama is running
        if !self.ollama_client.is_available() {
            return Err(color_eyre::eyre::eyre!(
                "Cannot connect to Ollama. Make sure it's running:\n  ollama serve"
            ));
        }

        // Check if the model exists
        match self.ollama_client.check_model(&agent.model) {
            Ok(true) => {
                let elapsed_ms = start.elapsed().as_millis();
                Ok(format!(
                    "Init in {}ms • {} [OLLAMA]",
                    elapsed_ms, agent.model
                ))
            }
            Ok(false) => Err(color_eyre::eyre::eyre!(
                "Model '{}' not found. Pull it first:\n  ollama pull {}",
                agent.model,
                agent.model
            )),
            Err(e) => Err(color_eyre::eyre::eyre!("Failed to check model: {}", e)),
        }
    }

    /// Sends a chat request to the agent
    pub fn chat(&self, agent: &Agent, messages: &[ChatMessage]) -> Result<String> {
        match agent.model_source {
            ModelSource::Ollama => self.ollama_client.chat(&agent.model, messages),
            ModelSource::VeniceAPI => {
                let api_key = self
                    .venice_api_key
                    .as_ref()
                    .ok_or_else(|| color_eyre::eyre::eyre!("Venice API key not configured"))?;
                crate::agents::venice::chat(api_key, &agent.model, messages)
            }
            ModelSource::GabAI => {
                let api_key = self
                    .gab_api_key
                    .as_ref()
                    .ok_or_else(|| color_eyre::eyre::eyre!("Gab AI key not configured"))?;
                crate::agents::gab_ai::chat(api_key, &self.gab_base_url, &agent.model, messages)
            }
        }
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        self.ollama_client.list_models()
    }

    pub fn set_venice_api_key(&mut self, api_key: String) {
        self.venice_api_key = Some(api_key);
    }

    pub fn set_gab_api_key(&mut self, api_key: String) {
        if api_key.trim().is_empty() {
            self.gab_api_key = None;
        } else {
            self.gab_api_key = Some(api_key);
        }
    }
}

/// A chat message for agent communication
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub images: Vec<String>,
}

/// Role of a message in the conversation
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            images: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            images: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            images: Vec::new(),
        }
    }
}
