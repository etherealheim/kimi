pub mod brave;
#[path = "gab-ai.rs"]
pub mod gab_ai;
pub mod ollama;
#[path = "openai-compat.rs"]
pub mod openai_compat;
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
    pub num_gpu: Option<i32>,
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
                    num_gpu: agent_config.num_gpu,
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
            self.venice_api_key
                .as_ref()
                .ok_or_else(|| color_eyre::eyre::eyre!("Venice API key not configured"))?;
            return Ok("Venice API ready".to_string());
        }
        if agent.model_source == ModelSource::GabAI {
            self.gab_api_key
                .as_ref()
                .ok_or_else(|| color_eyre::eyre::eyre!("Gab AI key not configured"))?;
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
            ModelSource::Ollama => self.ollama_client.chat(&agent.model, messages, agent.num_gpu),
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

    /// Sends a chat request with native tool calling support
    /// Venice API supports native tools; Ollama and Gab fall back to text-only response
    pub fn chat_with_tools(
        &self,
        agent: &Agent,
        messages: &[ChatMessage],
        tools: &[openai_compat::ToolDefinition],
    ) -> Result<openai_compat::ChatResponse> {
        match agent.model_source {
            ModelSource::VeniceAPI => {
                let api_key = self
                    .venice_api_key
                    .as_ref()
                    .ok_or_else(|| color_eyre::eyre::eyre!("Venice API key not configured"))?;
                crate::agents::venice::chat_with_tools(api_key, &agent.model, messages, tools)
            }
            // Ollama and Gab don't support native tool calling -- return text-only response
            ModelSource::Ollama | ModelSource::GabAI => {
                let content = self.chat(agent, messages)?;
                Ok(openai_compat::ChatResponse::text(content))
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
    /// Tool calls made by the assistant (for native tool calling)
    pub tool_calls: Option<Vec<openai_compat::ToolCallResponse>>,
    /// The ID of the tool call this message responds to (role = Tool)
    pub tool_call_id: Option<String>,
}

/// Role of a message in the conversation
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates an assistant message that contains tool calls (from native API response)
    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<openai_compat::ToolCallResponse>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Creates a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}
