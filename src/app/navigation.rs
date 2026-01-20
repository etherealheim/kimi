/// Generic navigation trait for list-like UI components
/// Eliminates duplication of next/previous item logic across modules
pub trait Navigable {
    /// Returns the total number of items in the list
    fn get_item_count(&self) -> usize;

    /// Returns the currently selected index
    fn get_selected_index(&self) -> usize;

    /// Sets the selected index
    fn set_selected_index(&mut self, index: usize);

    /// Moves to the next item (wraps around to start)
    fn next_item(&mut self) {
        let count = self.get_item_count();
        if count > 0 {
            let next = (self.get_selected_index() + 1) % count;
            self.set_selected_index(next);
        }
    }

    /// Moves to the previous item (wraps around to end)
    fn previous_item(&mut self) {
        let count = self.get_item_count();
        if count > 0 {
            let prev = if self.get_selected_index() == 0 {
                count - 1
            } else {
                self.get_selected_index() - 1
            };
            self.set_selected_index(prev);
        }
    }
}
