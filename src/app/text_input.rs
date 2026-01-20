/// Reusable text input component
/// Eliminates duplication of add/remove char logic across modules
#[derive(Debug, Clone)]
pub struct TextInput {
    content: String,
}

impl TextInput {
    /// Creates a new empty text input
    pub fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    /// Creates a text input with initial content
    pub fn with_content(content: String) -> Self {
        Self { content }
    }

    /// Adds a character to the input
    pub fn add_char(&mut self, character: char) {
        self.content.push(character);
    }

    /// Removes the last character from the input
    pub fn remove_char(&mut self) {
        self.content.pop();
    }

    /// Gets the current content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Clears the input
    pub fn clear(&mut self) {
        self.content.clear();
    }

    /// Checks if the input is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Sets the content directly
    pub fn set_content(&mut self, content: String) {
        self.content = content;
    }
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
