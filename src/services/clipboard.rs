use arboard::Clipboard;
use color_eyre::Result;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;
use std::process::Command;

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

    pub fn read_image_png(&mut self) -> Result<Vec<u8>> {
        let clipboard_result = self.read_image_png_from_arboard();
        if let Ok(bytes) = clipboard_result.as_ref() {
            return Ok(bytes.clone());
        }

        if let Ok(bytes) = read_image_png_external() {
            return Ok(bytes);
        }

        clipboard_result
    }

    fn read_image_png_from_arboard(&mut self) -> Result<Vec<u8>> {
        let clipboard = self.get_clipboard()?;
        let image = clipboard.get_image()?;
        let width = image.width as u32;
        let height = image.height as u32;
        let bytes = image.bytes.into_owned();
        let image_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, bytes)
            .ok_or_else(|| color_eyre::eyre::eyre!("Clipboard image data invalid"))?;
        let mut output = Vec::new();
        let mut cursor = Cursor::new(&mut output);
        DynamicImage::ImageRgba8(image_buffer).write_to(&mut cursor, ImageFormat::Png)?;
        Ok(output)
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

fn read_image_png_external() -> Result<Vec<u8>> {
    if let Ok(bytes) = run_clipboard_command("wl-paste", &["--type", "image/png"]) {
        return Ok(bytes);
    }
    if let Ok(bytes) = run_clipboard_command("xclip", &["-selection", "clipboard", "-t", "image/png", "-o"]) {
        return Ok(bytes);
    }
    Err(color_eyre::eyre::eyre!("Clipboard image unavailable"))
}

fn run_clipboard_command(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(color_eyre::eyre::eyre!("Clipboard command failed"));
    }
    if output.stdout.is_empty() {
        return Err(color_eyre::eyre::eyre!("Clipboard image empty"));
    }
    Ok(output.stdout)
}
