use crate::app::types::MenuItem;
use crate::app::{App, AppMode, Navigable};
use color_eyre::Result;

/// Command implementations
pub fn cmd_quit() -> Result<String> {
    Ok("Goodbye!".to_string())
}

impl App {
    pub fn open_command_menu(&mut self) {
        // Save the previous mode so we can return to it
        self.previous_mode = Some(self.mode.clone());
        self.mode = AppMode::CommandMenu;
        self.input.clear();
        self.selected_index = 0;
    }

    pub fn close_menu(&mut self) {
        self.mode = self.previous_mode.take().unwrap_or(AppMode::Chat);
        self.input.clear();
        self.selected_index = 0;
    }

    pub fn add_input_char(&mut self, character: char) {
        self.input.push(character);
        self.selected_index = 0;
    }

    pub fn remove_input_char(&mut self) {
        self.input.pop();
        self.selected_index = 0;
    }

    /// Returns filtered menu items based on current input
    #[must_use]
    pub fn filtered_items(&self) -> Vec<MenuItem> {
        if self.input.is_empty() {
            return self.menu_items.clone();
        }

        self.menu_items
            .iter()
            .filter(|item| {
                item.name
                    .to_lowercase()
                    .contains(&self.input.to_lowercase())
                    || item
                        .description
                        .to_lowercase()
                        .contains(&self.input.to_lowercase())
            })
            .cloned()
            .collect()
    }
}

// Implement Navigable for menu navigation
impl Navigable for App {
    fn get_item_count(&self) -> usize {
        self.filtered_items().len()
    }

    fn get_selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, index: usize) {
        self.selected_index = index;
    }
}
