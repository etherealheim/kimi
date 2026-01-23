use crate::app::types::ModelSelectionItem;
use crate::app::{App, AppMode, ModelSource, Navigable};
use color_eyre::Result;
use std::collections::HashMap;

impl App {
    fn persist_selected_model(&self, agent_name: &str, model_name: &str) -> Result<()> {
        let mut config = crate::config::Config::load()?;
        if agent_name == "embeddings" {
            config.embeddings.model = model_name.to_string();
            config.save()?;
        } else if let Some(agent_config) = config.agents.get_mut(agent_name) {
            agent_config.model = model_name.to_string();
            config.save()?;
        }
        Ok(())
    }

    pub fn refresh_available_models(&mut self) -> Result<()> {
        let manager = self
            .agent_manager
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("Agent manager not initialized"))?;

        let installed_models = manager.list_models()?;
        let venice_models = fetch_venice_models(&self.connect_venice_key);
        let gab_models = fetch_gab_models(&self.connect_gab_key);

        let mut available_models: HashMap<String, Vec<crate::app::AvailableModel>> =
            HashMap::new();
        let agent_order = ["embeddings", "translate", "chat", "routing"];
        for agent_name in agent_order {
            let mut models = build_ollama_models(agent_name, &installed_models);
            if agent_name != "routing" && agent_name != "embeddings" && agent_name != "translate" {
                if let Some(venice_models) = &venice_models {
                    for model_name in venice_models {
                        models.push(crate::app::AvailableModel {
                            name: model_name.clone(),
                            source: ModelSource::VeniceAPI,
                            is_available: true,
                        });
                    }
                }
                if agent_name == "chat"
                    && let Some(gab_models) = &gab_models
                {
                    for model_name in gab_models {
                        models.push(crate::app::AvailableModel {
                            name: model_name.clone(),
                            source: ModelSource::GabAI,
                            is_available: true,
                        });
                    }
                }
            }
            available_models.insert(agent_name.to_string(), models);
        }

        self.available_models = available_models;
        self.rebuild_menu_items();

        let mut reload_agent_name: Option<String> = None;
        let agent_order = ["embeddings", "translate", "chat"];
        for agent_name in agent_order {
            let selected = self
                .selected_models
                .entry(agent_name.to_string())
                .or_default();

            let has_available = self
                .available_models
                .get(agent_name)
                .is_some_and(|models| !models.is_empty());

            let should_reload = self
                .current_agent
                .as_ref()
                .is_some_and(|agent| agent.name == agent_name);

            if has_available {
                let first_available = self
                    .available_models
                    .get(agent_name)
                    .and_then(|models| models.first())
                    .map(|model| model.name.clone());

                let selected_name = selected.first().cloned();
                let is_selected_available = selected_name.as_ref().is_some_and(|name| {
                    self.available_models
                        .get(agent_name)
                        .is_some_and(|models| models.iter().any(|m| m.name == *name))
                });

                if !is_selected_available {
                    selected.clear();
                    if let Some(model_name) = first_available {
                        selected.push(model_name);
                    }
                    if should_reload {
                        reload_agent_name = Some(agent_name.to_string());
                    }
                }
            } else if !selected.is_empty() {
                selected.clear();
                if should_reload {
                    reload_agent_name = Some(agent_name.to_string());
                }
            }
        }

        if let Some(agent_name) = reload_agent_name {
            let _ = self.load_agent(&agent_name);
        }

        Ok(())
    }

    pub fn set_selected_model(&mut self, agent_name: &str, model_name: &str) -> Result<()> {
        let models = self
            .available_models
            .get(agent_name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Unknown agent '{}'", agent_name))?;

        let is_valid = models.iter().any(|model| model.name == model_name);
        if !is_valid {
            return Err(color_eyre::eyre::eyre!(
                "Model '{}' not available for agent '{}'",
                model_name,
                agent_name
            ));
        }

        let should_reload = self
            .current_agent
            .as_ref()
            .is_some_and(|agent| agent.name == agent_name);

        let selected = self
            .selected_models
            .entry(agent_name.to_string())
            .or_default();
        selected.clear();
        selected.push(model_name.to_string());
        let _ = self.persist_selected_model(agent_name, model_name);

        if should_reload {
            self.load_agent(agent_name)?;
        }

        Ok(())
    }

    pub fn open_model_selection(&mut self) {
        let _ = self.refresh_available_models();
        self.mode = AppMode::ModelSelection;
        self.model_selection_index = 0;
        self.build_model_selection_items();
    }

    pub fn close_model_selection(&mut self) {
        self.mode = AppMode::Chat; // Return to chat mode instead of Normal
        self.model_selection_index = 0;
        self.model_selection_items.clear();
    }

    fn build_model_selection_items(&mut self) {
        self.model_selection_items.clear();

        // Define agent order
        let agent_order = vec!["embeddings", "translate", "chat", "routing"];

        for agent_name in agent_order {
            if let Some(models) = self.available_models.get(agent_name) {
                for model_index in 0..models.len() {
                    self.model_selection_items.push(ModelSelectionItem {
                        agent_name: agent_name.to_string(),
                        model_index,
                    });
                }
            }
        }
    }

    pub fn toggle_model_selection(&mut self) {
        if let Some(item) = self.model_selection_items.get(self.model_selection_index).cloned()
            && let Some(models) = self.available_models.get(&item.agent_name)
            && let Some(model) = models.get(item.model_index)
        {
            let agent_name = item.agent_name.clone();
            let model_name = model.name.clone();
            let should_reload = self
                .current_agent
                .as_ref()
                .is_some_and(|agent| agent.name == agent_name);
            let selected = self
                .selected_models
                .entry(agent_name.clone())
                .or_default();

            if let Some(pos) = selected.iter().position(|x| x == &model_name) {
                selected.remove(pos);
                if should_reload && selected.is_empty()
                    && let Err(error) = self.load_agent(&agent_name)
                {
                    self.add_system_message(&format!("Failed to reload agent: {}", error));
                }
            } else {
                selected.clear();
                selected.push(model_name);
                let selected_name = selected.first().cloned();
                if let Some(selected_name) = selected_name {
                    let _ = self.persist_selected_model(&agent_name, &selected_name);
                }
                if should_reload
                    && let Err(error) = self.load_agent(&agent_name)
                {
                    self.add_system_message(&format!("Failed to reload agent: {}", error));
                }
            }
        }
    }
}

fn build_ollama_models(
    agent_name: &str,
    installed_models: &[String],
) -> Vec<crate::app::AvailableModel> {
    let mut models = Vec::new();
    let is_routing_agent = agent_name == "routing";
    let is_embeddings = agent_name == "embeddings";
    let is_translate = agent_name == "translate";
    
    for model_name in installed_models {
        let is_function_model = is_function_calling_model(model_name);
        let is_embedding_model = is_embedding_model(model_name);
        let is_translate_model = is_translate_model(model_name);
        
        // Routing agent: only function models
        if is_routing_agent && !is_function_model {
            continue;
        }
        if !is_routing_agent && is_function_model {
            continue;
        }
        
        // Embeddings: only embedding models
        if is_embeddings && !is_embedding_model {
            continue;
        }
        if !is_embeddings && is_embedding_model {
            continue;
        }
        
        // Translate: only translate models (translategemma)
        if is_translate && !is_translate_model {
            continue;
        }
        
        models.push(crate::app::AvailableModel {
            name: model_name.to_string(),
            source: ModelSource::Ollama,
            is_available: true,
        });
    }
    models
}

fn fetch_venice_models(api_key: &str) -> Option<Vec<String>> {
    if api_key.trim().is_empty() {
        return None;
    }
    crate::agents::venice::fetch_text_models(api_key).ok()
}

fn fetch_gab_models(api_key: &str) -> Option<Vec<String>> {
    if api_key.trim().is_empty() {
        return None;
    }
    Some(vec!["arya".to_string()])
}

fn is_function_calling_model(model_name: &str) -> bool {
    let lowered = model_name.to_lowercase();
    lowered.contains("function")
}

fn is_embedding_model(model_name: &str) -> bool {
    let lowered = model_name.to_lowercase();
    lowered.contains("embed") || lowered.contains("bge")
}

fn is_translate_model(model_name: &str) -> bool {
    let lowered = model_name.to_lowercase();
    lowered.contains("translategemma") || lowered.contains("translate")
}


// Navigation for model selection
pub struct ModelSelectionNavigable<'a> {
    app: &'a mut App,
}

impl<'a> ModelSelectionNavigable<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }
}

impl<'a> Navigable for ModelSelectionNavigable<'a> {
    fn get_item_count(&self) -> usize {
        self.app.model_selection_items.len()
    }

    fn get_selected_index(&self) -> usize {
        self.app.model_selection_index
    }

    fn set_selected_index(&mut self, index: usize) {
        self.app.model_selection_index = index;
    }
}

// Convenience methods for model selection navigation
impl App {
    pub fn next_model(&mut self) {
        ModelSelectionNavigable::new(self).next_item();
    }

    pub fn previous_model(&mut self) {
        ModelSelectionNavigable::new(self).previous_item();
    }
}
