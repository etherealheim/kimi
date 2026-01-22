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
    pub gab: GabConfig,
    #[serde(default)]
    pub brave: BraveConfig,
    #[serde(default)]
    pub obsidian: ObsidianConfig,
    #[serde(default)]
    pub personality: PersonalityConfig,
    pub agents: HashMap<String, AgentConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct LocalConfig {
    elevenlabs: Option<LocalElevenLabsConfig>,
    venice: Option<LocalApiConfig>,
    gab: Option<LocalApiConfig>,
    brave: Option<LocalApiConfig>,
    obsidian: Option<LocalObsidianConfig>,
}

#[derive(Debug, Deserialize)]
struct LocalElevenLabsConfig {
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalApiConfig {
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalObsidianConfig {
    vault_path: Option<String>,
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

/// Gab AI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GabConfig {
    pub api_key: String,
    pub base_url: String,
}

/// Brave Search configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BraveConfig {
    pub api_key: String,
}

/// Obsidian vault configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObsidianConfig {
    pub vault_path: String,
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

        // PERSONALITY: Base prompt is loaded from data/personalities/Kimi.md
        let kimi_personality =
            "Kimi base prompt is loaded from data/personalities/Kimi.md.";

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

        agents.insert(
            "routing".to_string(),
            AgentConfig {
                model: "functiongemma".to_string(),
                system_prompt: "Function calling router.".to_string(),
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
            gab: GabConfig {
                api_key: String::new(),
                base_url: crate::agents::gab_ai::default_base_url(),
            },
            brave: BraveConfig {
                api_key: String::new(),
            },
            obsidian: ObsidianConfig {
                vault_path: String::new(),
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
        let project_config_path = Self::project_config_path()?;
        let legacy_config_path = Self::legacy_config_path()?;

        let mut config = if project_config_path.exists() {
            let contents = fs::read_to_string(&project_config_path)?;
            toml::from_str(&contents)?
        } else if legacy_config_path.exists() {
            let contents = fs::read_to_string(&legacy_config_path)?;
            let config: Config = toml::from_str(&contents)?;
            config.save()?;
            config
        } else {
            // Create default config file
            let config = Config::default();
            config.save()?;
            config
        };

        if let Some(local) = Self::load_local_config()? {
            Self::apply_local_overrides(&mut config, &local);
        }
        Ok(config)
    }

    /// Saves configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_path = Self::project_config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let redacted = self.redacted_for_project();
        fs::write(&config_path, toml::to_string_pretty(&redacted)?)?;
        Ok(())
    }

    /// Returns the path to the configuration file
    pub fn project_config_path() -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;
        Ok(current_dir.join("config.toml"))
    }

    fn legacy_config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "kimi")
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    fn local_config_path() -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;
        Ok(current_dir.join("config.local.toml"))
    }

    fn load_local_config() -> Result<Option<LocalConfig>> {
        let path = Self::local_config_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(&path)?;
        let local = toml::from_str(&contents)?;
        Ok(Some(local))
    }

    fn apply_local_overrides(config: &mut Self, local: &LocalConfig) {
        if let Some(elevenlabs) = &local.elevenlabs {
            if let Some(api_key) = &elevenlabs.api_key {
                if !api_key.trim().is_empty() {
                    config.elevenlabs.api_key = api_key.clone();
                }
            }
        }
        if let Some(venice) = &local.venice {
            if let Some(api_key) = &venice.api_key {
                if !api_key.trim().is_empty() {
                    config.venice.api_key = api_key.clone();
                }
            }
        }
        if let Some(brave) = &local.brave {
            if let Some(api_key) = &brave.api_key {
                if !api_key.trim().is_empty() {
                    config.brave.api_key = api_key.clone();
                }
            }
        }
        if let Some(gab) = &local.gab {
            if let Some(api_key) = &gab.api_key {
                if !api_key.trim().is_empty() {
                    config.gab.api_key = api_key.clone();
                }
            }
        }
        if let Some(obsidian) = &local.obsidian {
            if let Some(vault_path) = &obsidian.vault_path {
                if !vault_path.trim().is_empty() {
                    config.obsidian.vault_path = vault_path.clone();
                }
            }
        }
    }

    fn redacted_for_project(&self) -> Self {
        let mut redacted = self.clone();
        redacted.elevenlabs.api_key = String::new();
        redacted.venice.api_key = String::new();
        redacted.gab.api_key = String::new();
        redacted.brave.api_key = String::new();
        redacted
    }
}
