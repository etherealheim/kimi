use color_eyre::Result;

/// Command implementations
pub fn cmd_quit() -> Result<String> {
    Ok("Goodbye!".to_string())
}
