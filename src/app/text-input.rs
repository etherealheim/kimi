/// Reusable text input component
/// Eliminates duplication of add/remove char logic across modules
#[derive(Debug, Clone)]
pub struct TextInput {
    content: String,
    cursor_index: usize,
}

impl TextInput {
    /// Creates a new empty text input
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor_index: 0,
        }
    }

    /// Creates a text input with initial content
    pub fn with_content(content: String) -> Self {
        let cursor_index = content.chars().count();
        Self {
            content,
            cursor_index,
        }
    }

    /// Adds a character to the input
    pub fn add_char(&mut self, character: char) {
        let insert_index = char_to_byte_index(&self.content, self.cursor_index);
        self.content.insert(insert_index, character);
        self.cursor_index = self.cursor_index.saturating_add(1);
    }

    /// Removes the last character from the input
    pub fn remove_char(&mut self) {
        if self.cursor_index == 0 {
            return;
        }
        let end_index = char_to_byte_index(&self.content, self.cursor_index);
        let start_index = char_to_byte_index(&self.content, self.cursor_index.saturating_sub(1));
        if start_index < end_index {
            self.content.replace_range(start_index..end_index, "");
            self.cursor_index = self.cursor_index.saturating_sub(1);
        }
    }

    /// Removes the character at the cursor (delete)
    pub fn delete_char(&mut self) {
        let length = self.content.chars().count();
        if self.cursor_index >= length {
            return;
        }
        let start_index = char_to_byte_index(&self.content, self.cursor_index);
        let end_index = char_to_byte_index(&self.content, self.cursor_index.saturating_add(1));
        if start_index < end_index {
            self.content.replace_range(start_index..end_index, "");
        }
    }

    /// Moves cursor left by one character
    pub fn move_left(&mut self) {
        self.cursor_index = self.cursor_index.saturating_sub(1);
    }

    /// Moves cursor right by one character
    pub fn move_right(&mut self) {
        let length = self.content.chars().count();
        if self.cursor_index < length {
            self.cursor_index += 1;
        }
    }

    /// Moves cursor to the start of the input
    pub fn move_to_start(&mut self) {
        self.cursor_index = 0;
    }

    /// Moves cursor to the end of the input
    pub fn move_to_end(&mut self) {
        self.cursor_index = self.content.chars().count();
    }

    /// Gets the current content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns cursor position in characters
    pub fn cursor_position(&self) -> usize {
        self.cursor_index
    }

    /// Clears the input
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor_index = 0;
    }

    /// Checks if the input is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Sets the content directly
    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.cursor_index = self.content.chars().count();
    }
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or_else(|| value.len(), |(index, _)| index)
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for TextInput {
    fn from(content: String) -> Self {
        Self::with_content(content)
    }
}

impl From<&str> for TextInput {
    fn from(content: &str) -> Self {
        Self::with_content(content.to_string())
    }
}
