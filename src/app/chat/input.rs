use crate::app::types::{ChatAttachment, ChatMessage, MessageRole};
use crate::app::App;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Datelike, Local};
use color_eyre::Result;
use std::path::{Path, PathBuf};

impl App {
    /// Adds a user message to the chat history with timestamp
    fn add_user_message_to_history(&mut self, message_content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            content: message_content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
        });
    }

    pub fn send_chat_message(&mut self) -> Result<()> {
        if self.chat_input.is_empty() {
            return Ok(());
        }

        if self.handle_convert_command()? {
            return Ok(());
        }

        if self.handle_download_command()? {
            return Ok(());
        }

        let user_message = self.cleaned_chat_input_with_attachments();
        self.chat_input.clear();
        self.reset_chat_scroll();

        if let Some(reply) = try_handle_date_question(&user_message) {
            self.add_assistant_message(&reply);
            return Ok(());
        }

        self.add_user_message_to_history(&user_message);
        self.is_loading = true;

        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;
        let mut messages = self.build_agent_messages(&agent.system_prompt);
        if !self.chat_attachments.is_empty() {
            let images = self.build_attachment_images()?;
            self.apply_images_to_last_user_message(&mut messages, images);
            self.chat_attachments.clear();
        }

        Self::spawn_agent_chat_thread(agent, manager, messages, agent_tx);

        Ok(())
    }

    pub fn add_chat_input_char(&mut self, character: char) {
        self.chat_input.add_char(character);
    }

    pub fn remove_chat_input_char(&mut self) {
        if self.remove_attachment_token_from_input() {
            return;
        }
        self.chat_input.remove_char();
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        let display_name = if self.personality_enabled {
            self.personality_name.clone()
        } else {
            None
        };
        self.chat_history.push(ChatMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name,
        });
    }

    pub fn handle_chat_paste(&mut self, text: &str) -> Result<()> {
        if self.try_add_attachment_from_paste(text)? {
            return Ok(());
        }
        for character in text.chars() {
            self.add_chat_input_char(character);
        }
        Ok(())
    }

    pub fn handle_chat_clipboard_image(&mut self) -> Result<()> {
        match self.add_clipboard_image_attachment() {
            Ok(true) => self.show_status_toast("IMAGE ADDED"),
            Ok(false) => self.show_status_toast("NO IMAGE"),
            Err(error) => {
                self.show_status_toast("NO IMAGE");
                self.add_system_message(&format!("Clipboard image error: {}", error));
            }
        }
        Ok(())
    }

    pub fn handle_command_menu_paste(&mut self, text: &str) -> Result<bool> {
        if self.try_add_image_attachment_from_text(text)? {
            self.show_status_toast("IMAGE ADDED");
            self.close_menu();
            return Ok(true);
        }
        Ok(false)
    }

    fn build_attachment_images(&mut self) -> Result<Vec<String>> {
        let mut images = Vec::new();
        for attachment in &self.chat_attachments {
            match attachment {
                ChatAttachment::FilePath { path, .. } => {
                    let bytes = std::fs::read(path)?;
                    images.push(STANDARD.encode(bytes));
                }
                ChatAttachment::ClipboardImage { png_bytes, .. } => {
                    images.push(STANDARD.encode(png_bytes));
                }
            }
        }
        Ok(images)
    }

    fn apply_images_to_last_user_message(
        &self,
        messages: &mut [crate::agents::ChatMessage],
        images: Vec<String>,
    ) {
        if let Some(last) = messages.last_mut()
            && last.role == crate::agents::MessageRole::User
        {
            last.images = images;
        }
    }

    fn cleaned_chat_input_with_attachments(&mut self) -> String {
        let content = remove_attachment_tokens(self.chat_input.content());
        let mut cleaned_parts = Vec::new();
        for part in content.split_whitespace() {
            if let Some(path) = parse_image_path(part) {
                let _ = self.add_image_attachment_from_path(&path);
                continue;
            }
            cleaned_parts.push(part);
        }
        cleaned_parts.join(" ")
    }

    fn remove_attachment_token_from_input(&mut self) -> bool {
        let content = self.chat_input.content();
        for (index, attachment) in self.chat_attachments.iter().enumerate().rev() {
            let token = attachment.token();
            if content.ends_with(token) {
                let new_len = content.len().saturating_sub(token.len());
                self.chat_input.set_content(content[..new_len].to_string());
                self.chat_attachments.remove(index);
                return true;
            }
            let spaced_token = format!(" {}", token);
            if content.ends_with(&spaced_token) {
                let new_len = content.len().saturating_sub(spaced_token.len());
                self.chat_input.set_content(content[..new_len].to_string());
                self.chat_attachments.remove(index);
                return true;
            }
        }
        false
    }

    fn try_add_attachment_from_paste(&mut self, text: &str) -> Result<bool> {
        let trimmed = text.trim();
        if self.try_add_image_attachment_from_text(trimmed)? {
            return Ok(true);
        }

        match self.add_clipboard_image_attachment() {
            Ok(true) => Ok(true),
            Ok(false) => Ok(false),
            Err(error) => {
                self.add_system_message(&format!("Clipboard image error: {}", error));
                Ok(true)
            }
        }
    }

    fn add_image_attachment_from_path(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        if !is_supported_image_path(path) {
            return Ok(());
        }
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_string();
        let token = make_attachment_token(&label);
        self.chat_attachments.push(ChatAttachment::FilePath {
            token: token.clone(),
            path: path.to_path_buf(),
        });
        self.append_attachment_token(&token);
        Ok(())
    }

    fn append_attachment_token(&mut self, token: &str) {
        let content = self.chat_input.content();
        let spaced = if content.is_empty() {
            token.to_string()
        } else {
            format!("{} {}", content, token)
        };
        self.chat_input.set_content(spaced);
    }

    pub(crate) fn try_add_image_attachment_from_text(&mut self, text: &str) -> Result<bool> {
        let trimmed = text.trim();
        let mut did_add = false;
        for line in trimmed.lines() {
            if let Some(path) = parse_image_path(line.trim()) {
                self.add_image_attachment_from_path(&path)?;
                did_add = true;
            }
        }
        Ok(did_add)
    }

    fn add_clipboard_image_attachment(&mut self) -> Result<bool> {
        let png_bytes = self.clipboard_service.read_image_png()?;
        if png_bytes.is_empty() {
            return Ok(false);
        }
        let label = format!("clipboard-{}", self.next_attachment_id);
        let token = make_attachment_token(&label);
        self.next_attachment_id += 1;
        self.chat_attachments.push(ChatAttachment::ClipboardImage {
            token: token.clone(),
            png_bytes,
        });
        self.append_attachment_token(&token);
        Ok(true)
    }
}

fn parse_image_path(input: &str) -> Option<PathBuf> {
    let mut candidate = input.trim().trim_matches('"').to_string();
    if candidate.starts_with("file://") {
        candidate = candidate.trim_start_matches("file://").to_string();
    }
    if candidate.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            candidate = format!("{}/{}", home, candidate.trim_start_matches("~/"));
        }
    }
    if candidate.starts_with("home/") {
        candidate = format!("/{}", candidate);
    } else if let Ok(user) = std::env::var("USER") {
        if candidate.starts_with(&format!("{}/", user)) {
            candidate = format!("/home/{}", candidate);
        }
    }
    if !candidate.starts_with('/') && !candidate.starts_with("~/") && candidate.contains('/') {
        if let Ok(home) = std::env::var("HOME") {
            candidate = format!("{}/{}", home, candidate);
        }
    }
    if candidate.is_empty() {
        return None;
    }
    let path = PathBuf::from(candidate);
    if path.exists() {
        return Some(path);
    }
    None
}

fn is_supported_image_path(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    matches!(
        extension.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff" | "gif"
    )
}

fn make_attachment_token(label: &str) -> String {
    let sanitized = label.replace(']', ")").replace('[', "(");
    format!("[[image:{}]]", sanitized)
}

fn remove_attachment_tokens(content: &str) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < content.len() {
        if let Some(start) = content[index..].find("[[image:") {
            let start_index = index + start;
            output.push_str(&content[index..start_index]);
            if let Some(end) = content[start_index..].find("]]") {
                index = start_index + end + 2;
                continue;
            }
        }
        output.push_str(&content[index..]);
        break;
    }
    output
}

fn try_handle_date_question(input: &str) -> Option<String> {
    let lowered = input.trim().to_lowercase();
    let today = chrono::Local::now().date_naive();
    if lowered.contains("day after tomorrow") {
        let date = today + chrono::Duration::days(2);
        return Some(format!("The day after tomorrow is {}.", date.format("%A, %B %d, %Y")));
    }
    if lowered.contains("today") {
        return Some(format!("Today is {}.", today.format("%A, %B %d, %Y")));
    }
    if lowered.contains("tomorrow") {
        let tomorrow = today + chrono::Duration::days(1);
        return Some(format!(
            "Tomorrow is {}.",
            tomorrow.format("%A, %B %d, %Y")
        ));
    }
    if lowered.contains("yesterday") {
        let yesterday = today - chrono::Duration::days(1);
        return Some(format!(
            "Yesterday was {}.",
            yesterday.format("%A, %B %d, %Y")
        ));
    }
    if let Some(days) = parse_day_offset(&lowered) {
        let date = today + chrono::Duration::days(days.into());
        if days >= 0 {
            return Some(format!(
                "In {} days it will be {}.",
                days,
                date.format("%A, %B %d, %Y")
            ));
        }
        return Some(format!(
            "{} days ago was {}.",
            days.abs(),
            date.format("%A, %B %d, %Y")
        ));
    }
    if let Some(date) = parse_weekday_reference(&lowered, today) {
        return Some(format!(
            "That is {}.",
            date.format("%A, %B %d, %Y")
        ));
    }
    None
}

fn parse_day_offset(text: &str) -> Option<i64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for window in tokens.windows(3) {
        if let [number, "days", "ago"] = window {
            if let Ok(value) = number.parse::<i64>() {
                return Some(-value);
            }
        }
        if let [number, "days", "from"] = window {
            if let Ok(value) = number.parse::<i64>() {
                return Some(value);
            }
        }
    }
    for window in tokens.windows(2) {
        if let [number, "days"] = window {
            if let Ok(value) = number.parse::<i64>() {
                if text.contains("in ") {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn parse_weekday_reference(
    text: &str,
    today: chrono::NaiveDate,
) -> Option<chrono::NaiveDate> {
    let weekday = parse_weekday(text)?;
    let today_weekday = today.weekday();
    let mut delta = weekday.num_days_from_monday() as i64
        - today_weekday.num_days_from_monday() as i64;

    if text.contains("next ") {
        if delta <= 0 {
            delta += 7;
        }
    } else if text.contains("this ") {
        if delta < 0 {
            delta += 7;
        }
    } else if delta <= 0 {
        delta += 7;
    }

    Some(today + chrono::Duration::days(delta))
}

fn parse_weekday(text: &str) -> Option<chrono::Weekday> {
    if text.contains("monday") {
        return Some(chrono::Weekday::Mon);
    }
    if text.contains("tuesday") {
        return Some(chrono::Weekday::Tue);
    }
    if text.contains("wednesday") {
        return Some(chrono::Weekday::Wed);
    }
    if text.contains("thursday") {
        return Some(chrono::Weekday::Thu);
    }
    if text.contains("friday") {
        return Some(chrono::Weekday::Fri);
    }
    if text.contains("saturday") {
        return Some(chrono::Weekday::Sat);
    }
    if text.contains("sunday") {
        return Some(chrono::Weekday::Sun);
    }
    None
}
