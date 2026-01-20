use color_eyre::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ollama: OllamaConfig,
    pub elevenlabs: ElevenLabsConfig,
    #[serde(default)]
    pub venice: VeniceConfig,
    #[serde(default)]
    pub brave: BraveConfig,
    #[serde(default)]
    pub personality: PersonalityConfig,
    pub agents: HashMap<String, AgentConfig>,
}

/// Ollama backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub url: String,
}

/// ElevenLabs TTS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenLabsConfig {
    pub api_key: String,
    pub voice_id: String,
    pub model: String,
}

/// Venice AI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VeniceConfig {
    pub api_key: String,
}

/// Brave Search configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BraveConfig {
    pub api_key: String,
}

/// Personality configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonalityConfig {
    pub selected: String,
}

/// Agent-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub model: String,
    pub system_prompt: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut agents = HashMap::new();

        // PERSONALITY: Kimi is helpful, concise, and friendly
        // Edit this system prompt to change Kimi's personality across all agents
        let kimi_personality = "You are Kimi, a helpful AI assistant. Be concise, friendly, and direct. \
            Keep responses short and to the point unless asked for details.";

        agents.insert(
            "translate".to_string(),
            AgentConfig {
                model: "translategemma:latest".to_string(),
                system_prompt: format!(
                    "{} You specialize in translation between languages.",
                    kimi_personality
                ),
            },
        );

        agents.insert(
            "chat".to_string(),
            AgentConfig {
                model: "gemma3:12b".to_string(),
                system_prompt: kimi_personality.to_string(),
            },
        );

        Self {
            ollama: OllamaConfig {
                url: "http://localhost:11434".to_string(),
            },
            elevenlabs: ElevenLabsConfig {
                api_key: "your_api_key_here".to_string(),
                voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
                model: "eleven_monolingual_v1".to_string(),
            },
            venice: VeniceConfig {
                api_key: String::new(),
            },
            brave: BraveConfig {
                api_key: String::new(),
            },
            personality: PersonalityConfig {
                selected: "Casca".to_string(),
            },
            agents,
        }
    }
}

impl Config {
    /// Loads configuration from disk or creates default if not found
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            // Create default config file
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let contents = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Saves configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Returns the path to the configuration file
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "kimi")
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }
}
