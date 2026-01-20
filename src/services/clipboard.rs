use arboard::Clipboard;
use color_eyre::Result;

pub struct ClipboardService {
    clipboard: Option<Clipboard>,
}

impl ClipboardService {
    pub fn new() -> Self {
        Self {
            clipboard: Clipboard::new().ok(),
        }
    }

    pub fn copy_text(&mut self, text: &str) -> Result<()> {
        let clipboard = self.get_clipboard()?;
        clipboard.set_text(text.to_string())?;
        Ok(())
    }

    fn get_clipboard(&mut self) -> Result<&mut Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = Some(Clipboard::new()?);
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| color_eyre::eyre::eyre!("Clipboard unavailable"))
    }
}
