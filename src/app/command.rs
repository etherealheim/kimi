use crate::app::types::MenuItem;
use crate::app::{App, AppMode, Navigable};
use crate::services::fuzzy_score;
use color_eyre::Result;

/// Minimum fuzzy score threshold for a match to be included
const FUZZY_MATCH_THRESHOLD: f64 = 0.3;

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

    /// Returns filtered menu items based on current input using fuzzy matching.
    /// Results are sorted by match quality (best matches first).
    #[must_use]
    pub fn filtered_items(&self) -> Vec<MenuItem> {
        if self.input.is_empty() {
            return self.menu_items.clone();
        }

        let query = &self.input;

        // Score each item and collect matches above threshold
        let mut scored_items: Vec<(MenuItem, f64)> = self
            .menu_items
            .iter()
            .filter_map(|item| {
                let score = calculate_menu_item_score(query, item);
                if score >= FUZZY_MATCH_THRESHOLD {
                    Some((item.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending (best matches first)
        scored_items.sort_by(|first, second| {
            second
                .1
                .partial_cmp(&first.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Extract just the items
        scored_items.into_iter().map(|(item, _)| item).collect()
    }
}

/// Calculate a fuzzy match score for a menu item.
/// Takes the best score from name and description matches,
/// with a slight boost for name matches.
fn calculate_menu_item_score(query: &str, item: &MenuItem) -> f64 {
    let name_score = fuzzy_score(query, &item.name);
    let description_score = fuzzy_score(query, &item.description);

    // Boost name matches slightly since they're more relevant
    let boosted_name_score = name_score * 1.1;

    boosted_name_score.max(description_score).min(1.0)
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
