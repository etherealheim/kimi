use crate::app::{App, AppMode, Navigable, TextInput};
use crate::config::Config;
use color_eyre::Result;

impl App {
    pub fn open_personality_menu(&mut self) -> Result<()> {
        self.mode = AppMode::PersonalitySelection;
        self.personality_create_input.clear();
        self.reload_personality_items()?;
        Ok(())
    }

    pub fn close_personality_menu(&mut self) {
        self.mode = AppMode::Chat;
        self.personality_create_input.clear();
    }

    pub fn open_personality_create(&mut self) {
        self.mode = AppMode::PersonalityCreate;
        self.personality_create_input = TextInput::new();
    }

    pub fn add_personality_char(&mut self, character: char) {
        self.personality_create_input.add_char(character);
    }

    pub fn remove_personality_char(&mut self) {
        self.personality_create_input.remove_char();
    }

    pub fn create_personality(&mut self) -> Result<()> {
        let name = self.personality_create_input.content().trim().to_string();
        if name.is_empty() {
            self.add_system_message("Personality name cannot be empty");
            return Ok(());
        }

        crate::services::personality::create_personality(&name)?;
        self.personality_create_input.clear();
        self.reload_personality_items()?;
        self.set_active_personality(&name)?;
        self.mode = AppMode::PersonalitySelection;
        Ok(())
    }

    pub fn edit_selected_personality(&mut self) -> Result<()> {
        if self.personality_selected_index == 1 {
            if let Err(error) = crate::services::memories::open_memories_in_new_terminal() {
                crate::services::memories::open_memories_in_place()?;
                self.add_system_message(&format!("Memories editor error: {}", error));
            }
            return Ok(());
        }
        let name = if self.personality_selected_index == 0 {
            crate::services::personality::my_personality_name()
        } else {
            self.personality_items
                .get(self.personality_selected_index.saturating_sub(2))
                .cloned()
                .unwrap_or_else(crate::services::personality::default_personality_name)
        };
        if let Err(error) = crate::services::personality::open_personality_in_new_terminal(&name) {
            crate::services::personality::open_personality_in_place(&name)?;
            self.add_system_message(&format!("Personality editor error: {}", error));
        }
        Ok(())
    }

    pub fn delete_selected_personality(&mut self) -> Result<()> {
        if self.personality_selected_index <= 1 {
            self.add_system_message("This entry cannot be deleted");
            return Ok(());
        }
        if self.personality_items.len() <= 1 {
            self.add_system_message("Cannot delete the last personality");
            return Ok(());
        }

        let name = self
            .personality_items
            .get(self.personality_selected_index.saturating_sub(2))
            .cloned()
            .unwrap_or_else(crate::services::personality::default_personality_name);

        crate::services::personality::delete_personality(&name)?;
        self.reload_personality_items()?;

        if self.personality_name.as_deref() == Some(&name) {
            if let Some(first) = self.personality_items.first().cloned() {
                self.set_active_personality(&first)?;
            }
        }

        let total_items = self.personality_items.len() + 2;
        if self.personality_selected_index >= total_items && self.personality_selected_index > 0 {
            self.personality_selected_index = total_items.saturating_sub(1);
        }

        Ok(())
    }

    pub fn select_personality(&mut self) -> Result<()> {
        if self.personality_selected_index == 0 {
            return Ok(());
        }
        if self.personality_selected_index == 1 {
            if let Err(error) = crate::services::memories::open_memories_in_new_terminal() {
                crate::services::memories::open_memories_in_place()?;
                self.add_system_message(&format!("Memories editor error: {}", error));
            }
            return Ok(());
        }
        if let Some(name) = self
            .personality_items
            .get(self.personality_selected_index.saturating_sub(2))
            .cloned()
        {
            self.set_active_personality(&name)?;
        }
        Ok(())
    }

    pub fn reload_personality_items(&mut self) -> Result<()> {
        let _ = crate::services::personality::ensure_my_personality();
        let mut items = crate::services::personality::list_personalities()?;
        items.sort();
        self.personality_items = items;

        if let Some(active) = &self.personality_name {
            if let Some(index) = self
                .personality_items
                .iter()
                .position(|name| name == active)
            {
                self.personality_selected_index = index + 2;
                return Ok(());
            }
        }

        self.personality_selected_index = if self.personality_items.is_empty() { 0 } else { 2 };
        if let Some(first) = self.personality_items.first().cloned() {
            self.personality_name = Some(first);
        }
        Ok(())
    }

    fn set_active_personality(&mut self, name: &str) -> Result<()> {
        self.personality_name = Some(name.to_string());
        self.personality_text = None;

        if let Ok(mut config) = Config::load() {
            config.personality.selected = name.to_string();
            let _ = config.save();
        }
        self.show_status_toast("PERSONALITY SET");
        Ok(())
    }
}

pub struct PersonalityNavigable<'a> {
    app: &'a mut App,
}

impl<'a> PersonalityNavigable<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }
}

impl<'a> Navigable for PersonalityNavigable<'a> {
    fn get_item_count(&self) -> usize {
        self.app.personality_items.len() + 2
    }

    fn get_selected_index(&self) -> usize {
        self.app.personality_selected_index
    }

    fn set_selected_index(&mut self, index: usize) {
        self.app.personality_selected_index = index;
    }
}

impl App {
    pub fn next_personality(&mut self) {
        PersonalityNavigable::new(self).next_item();
    }

    pub fn previous_personality(&mut self) {
        PersonalityNavigable::new(self).previous_item();
    }
}
