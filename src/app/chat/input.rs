use crate::app::types::{ChatAttachment, ChatMessage, MessageRole};
use crate::app::App;
use crate::app::chat::agent::intent::classify_query;
use crate::services::weather::WeatherService;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Datelike, Local};
use color_eyre::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn query_is_notes_follow_up(query: &str) -> bool {
    let lowered = query.to_lowercase();
    let note_follow_up_terms = [
        "show", "display", "bring", "give me", "raw", "full", "complete", 
        "content", "note", "notes", "it", "that", "them"
    ];
    note_follow_up_terms.iter().any(|term| lowered.contains(term))
}

impl App {
    /// Adds a user message to the chat history with timestamp
    fn add_user_message_to_history(&mut self, message_content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            content: message_content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
            context_usage: None,
        });
    }
    
    /// Retrieves relevant messages from storage using App's existing connection
    fn retrieve_messages_for_query(&self, query: &str) -> Vec<crate::storage::RetrievedMessage> {
        let Some(storage) = &self.storage else {
            return Vec::new();
        };
        let Some(runtime) = self.storage_runtime() else {
            return Vec::new();
        };
        
        let embeddings_config = crate::config::Config::load()
            .map(|c| c.embeddings)
            .unwrap_or_default();
        
        runtime.block_on(async {
            crate::services::retrieval::retrieve_relevant_messages(
                storage,
                query,
                embeddings_config.max_retrieved_messages,
                embeddings_config.similarity_threshold,
            ).await.unwrap_or_default()
        })
    }

    pub fn send_chat_message(&mut self) -> Result<()> {
        if self.chat_input.is_empty() {
            return Ok(());
        }

        let command_content = self.chat_input.content().trim().to_string();
        if self.handle_convert_command()? {
            if !command_content.is_empty() {
                self.add_user_message_to_history(&command_content);
            }
            return Ok(());
        }

        if self.handle_download_command()? {
            if !command_content.is_empty() {
                self.add_user_message_to_history(&command_content);
            }
            return Ok(());
        }

        let user_message = self.cleaned_chat_input_with_attachments();
        self.chat_input.clear();
        self.reset_chat_scroll();
        self.add_user_message_to_history(&user_message);

        if let Some(action) = select_fast_path_action(&user_message)? {
            self.add_assistant_message(&action.into_reply());
            return Ok(());
        }

        self.is_loading = true;
        let intent = classify_query(&user_message);
        let search_request = SearchStateRequest {
            query: &user_message,
            intent,
        };
        let is_profile_query = crate::services::retrieval::is_profile_query(&user_message);
        self.is_searching = !is_profile_query && self.should_mark_searching(search_request);
        self.is_retrieving = should_mark_retrieving(&user_message);
        let is_fetching_notes = crate::app::chat::agent::obsidian::should_fetch_obsidian_for_intent(
            &self.connect_obsidian_vault,
            &user_message,
            intent,
        );
        self.is_fetching_notes = is_fetching_notes;
        self.is_analyzing = !self.chat_attachments.is_empty();
        
        // Clear cached notes if query is not about notes/follow-up
        if !is_fetching_notes && !query_is_notes_follow_up(&user_message) {
            self.cached_obsidian_notes = None;
        }

        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;
        
        // Do retrieval BEFORE spawning thread (while we have access to App's storage)
        let pre_retrieved = self.retrieve_messages_for_query(&user_message);
        
        let snapshot = crate::app::chat::agent::ChatBuildSnapshot {
            system_prompt: agent.system_prompt.clone(),
            chat_history: self.chat_history.clone(),
            personality_enabled: self.personality_enabled,
            personality_text: self.personality_text.clone(),
            personality_name: self.personality_name.clone(),
            connect_obsidian_vault: self.connect_obsidian_vault.clone(),
            connect_brave_key: self.connect_brave_key.clone(),
            pre_retrieved_messages: pre_retrieved,
            cached_obsidian_notes: self.cached_obsidian_notes.clone(),
        };
        let attachments = self.chat_attachments.clone();
        self.chat_attachments.clear();

        std::thread::spawn(move || {
            let build_result = crate::app::chat::agent::build_agent_messages_from_snapshot(
                snapshot, &agent, &manager,
            );
            
            // Send notes for caching if fetched
            if let Some((query, notes)) = build_result.notes_to_cache {
                let _ = agent_tx.send(crate::app::AgentEvent::CacheObsidianNotes { query, notes });
            }
            
            if let Some(response) = build_result.forced_response {
                let _ = agent_tx.send(crate::app::AgentEvent::ResponseWithContext {
                    response,
                    context_usage: build_result.context_usage,
                });
                return;
            }
            // If search had an issue, notify the user but still proceed with the agent request
            // This allows the agent to respond even when search fails
            if let Some(notice) = &build_result.pending_search_notice {
                let _ = agent_tx.send(crate::app::AgentEvent::SystemMessage(notice.clone()));
            }
            let mut messages = build_result.messages;
            if let Ok(images) = build_attachment_images_from_attachments(&attachments) {
                apply_images_to_last_user_message(&mut messages, images);
            }
            App::spawn_agent_chat_thread_with_context(
                agent,
                manager,
                messages,
                build_result.system_context,
                build_result.should_verify,
                agent_tx,
                build_result.context_usage,
            );
        });

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

    pub fn delete_chat_input_char(&mut self) {
        self.chat_input.delete_char();
    }

    pub fn move_chat_input_left(&mut self) {
        self.chat_input.move_left();
    }

    pub fn move_chat_input_right(&mut self) {
        self.chat_input.move_right();
    }

    pub fn move_chat_input_start(&mut self) {
        self.chat_input.move_to_start();
    }

    pub fn move_chat_input_end(&mut self) {
        self.chat_input.move_to_end();
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            display_name: None,
            context_usage: None,
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
            context_usage: None,
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

    fn should_mark_searching(&self, request: SearchStateRequest<'_>) -> bool {
        if self.connect_brave_key.trim().is_empty() {
            return false;
        }
        crate::app::chat::agent::search::should_mark_searching_for_intent(
            request.query,
            request.intent,
        )
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
    if candidate.starts_with("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        candidate = format!("{}/{}", home, candidate.trim_start_matches("~/"));
    }
    if candidate.starts_with("home/") {
        candidate = format!("/{}", candidate);
    } else if let Ok(user) = std::env::var("USER")
        && candidate.starts_with(&format!("{}/", user))
    {
        candidate = format!("/home/{}", candidate);
    }
    if !candidate.starts_with('/') && !candidate.starts_with("~/") && candidate.contains('/')
        && let Ok(home) = std::env::var("HOME")
    {
        candidate = format!("{}/{}", home, candidate);
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

fn build_attachment_images_from_attachments(attachments: &[ChatAttachment]) -> Result<Vec<String>> {
    let mut images = Vec::new();
    for attachment in attachments {
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
    messages: &mut [crate::agents::ChatMessage],
    images: Vec<String>,
) {
    if let Some(last) = messages.last_mut()
        && last.role == crate::agents::MessageRole::User
    {
        last.images = images;
    }
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
    if !should_handle_date_question(&lowered) {
        return None;
    }
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
        let date = today + chrono::Duration::days(days);
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

#[derive(Debug, Clone)]
enum FastPathAction {
    Weather(String),
    Time(String),
    Date(String),
}

impl FastPathAction {
    fn into_reply(self) -> String {
        match self {
            FastPathAction::Weather(reply) => reply,
            FastPathAction::Time(reply) => reply,
            FastPathAction::Date(reply) => reply,
        }
    }
}

fn select_fast_path_action(input: &str) -> Result<Option<FastPathAction>> {
    if let Some(reply) = try_handle_weather_question(input)? {
        return Ok(Some(FastPathAction::Weather(reply)));
    }
    if let Some(reply) = try_handle_time_question(input) {
        return Ok(Some(FastPathAction::Time(reply)));
    }
    if let Some(reply) = try_handle_date_question(input) {
        return Ok(Some(FastPathAction::Date(reply)));
    }
    Ok(None)
}

struct SearchStateRequest<'a> {
    query: &'a str,
    intent: crate::app::chat::agent::intent::QueryIntent,
}

#[derive(Debug, Deserialize)]
struct WeatherSnapshot {
    location: String,
    time: String,
    temperature_c: f32,
    wind_kph: f32,
}

fn try_handle_weather_question(input: &str) -> Result<Option<String>> {
    let lowered = input.trim().to_lowercase();
    if !should_handle_weather_question(&lowered) {
        return Ok(None);
    }
    if references_other_location(&lowered) {
        return Ok(Some(
            "I can only fetch current weather for Prague right now.".to_string(),
        ));
    }
    let service = WeatherService::new();
    match service.fetch_current_weather_json() {
        Ok(payload) => match serde_json::from_str::<WeatherSnapshot>(&payload) {
            Ok(snapshot) => Ok(Some(format_weather_snapshot(&snapshot))),
            Err(_) => Ok(Some(
                "I couldn't read the weather data just now.".to_string(),
            )),
        },
        Err(_) => Ok(Some(
            "I couldn't fetch the current weather right now.".to_string(),
        )),
    }
}

fn format_weather_snapshot(snapshot: &WeatherSnapshot) -> String {
    let temperature = format!("{:.1}", snapshot.temperature_c);
    let wind = format!("{:.0}", snapshot.wind_kph);
    format!(
        "Current weather in {}: {}°C, wind {} km/h (as of {}).",
        snapshot.location, temperature, wind, snapshot.time
    )
}

fn should_handle_weather_question(lowered: &str) -> bool {
    if lowered.is_empty() {
        return false;
    }
    let weather_terms = [
        "weather",
        "forecast",
        "temperature",
        "temp",
        "rain",
        "snow",
        "wind",
        "humidity",
    ];
    if !contains_any(lowered, &weather_terms) {
        return false;
    }
    let question_prefixes = [
        "what ",
        "when ",
        "which ",
        "is ",
        "does ",
        "do ",
        "tell me",
        "can you",
        "could you",
    ];
    let looks_like_question =
        lowered.contains('?') || question_prefixes.iter().any(|prefix| lowered.starts_with(prefix));
    looks_like_question || lowered.starts_with("weather") || lowered.starts_with("forecast")
}

fn references_other_location(lowered: &str) -> bool {
    let location_markers = [" in ", " at ", " for ", " near "];
    let mentions_location = location_markers
        .iter()
        .any(|marker| lowered.contains(marker));
    let mentions_prague = lowered.contains("prague") || lowered.contains("praha");
    mentions_location && !mentions_prague
}

fn try_handle_time_question(input: &str) -> Option<String> {
    let lowered = input.trim().to_lowercase();
    if !should_handle_time_question(&lowered) {
        return None;
    }
    let now = chrono::Local::now();
    let timezone = now.format("%Z").to_string();
    if timezone.trim().is_empty() {
        return Some(format!("It's {}.", now.format("%H:%M:%S")));
    }
    Some(format!(
        "It's {} {}.",
        now.format("%H:%M:%S"),
        timezone
    ))
}

fn should_handle_date_question(lowered: &str) -> bool {
    if lowered.is_empty() {
        return false;
    }
    let question_prefixes = [
        "what ",
        "when ",
        "which ",
        "is ",
        "does ",
        "do ",
        "tell me",
        "can you",
        "could you",
    ];
    let looks_like_question =
        lowered.contains('?') || question_prefixes.iter().any(|prefix| lowered.starts_with(prefix));
    if !looks_like_question {
        return false;
    }
    let explicit = [
        "what day",
        "what date",
        "which day",
        "what's the date",
        "what is the date",
        "what day is",
        "what's today",
        "what is today",
        "today's date",
        "tomorrow's date",
        "yesterday's date",
    ];
    if contains_any(lowered, &explicit) {
        return true;
    }
    let date_nouns = [
        " day",
        " day?",
        " day.",
        " day!",
        " date",
        " date?",
        " date.",
        " date!",
        " weekday",
    ];
    let uses_date_noun = contains_any(lowered, &date_nouns);
    let subject_forms = [
        "today is",
        "tomorrow is",
        "yesterday was",
        "day after tomorrow is",
        "day before yesterday was",
    ];
    let uses_subject_form = contains_any(lowered, &subject_forms);
    uses_date_noun || uses_subject_form
}

fn should_handle_time_question(lowered: &str) -> bool {
    if lowered.is_empty() {
        return false;
    }
    let question_prefixes = [
        "what ",
        "when ",
        "which ",
        "is ",
        "does ",
        "do ",
        "tell me",
        "can you",
        "could you",
    ];
    let looks_like_question =
        lowered.contains('?') || question_prefixes.iter().any(|prefix| lowered.starts_with(prefix));
    if !looks_like_question {
        return false;
    }
    let explicit = [
        "what time",
        "what's the time",
        "what is the time",
        "current time",
        "time is it",
        "time now",
    ];
    let time_terms = [" time", " clock", " timezone"];
    contains_any(lowered, &explicit) || contains_any(lowered, &time_terms)
}

fn parse_day_offset(text: &str) -> Option<i64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for window in tokens.windows(3) {
        if let [number, "days", "ago"] = window
            && let Ok(value) = number.parse::<i64>()
        {
            return Some(-value);
        }
        if let [number, "days", "from"] = window
            && let Ok(value) = number.parse::<i64>()
        {
            return Some(value);
        }
    }
    for window in tokens.windows(2) {
        if let [number, "days"] = window
            && let Ok(value) = number.parse::<i64>()
            && text.contains("in ")
        {
            return Some(value);
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

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// Determines if the query might trigger memory retrieval
fn should_mark_retrieving(query: &str) -> bool {
    let lowered = query.to_lowercase();
    let memory_triggers = [
        "what do i like",
        "what do i love",
        "what do i prefer",
        "what did i say",
        "what did i tell",
        "what did i mention",
        "what have i said",
        "do i like",
        "do i love",
        "do i prefer",
        "did i say",
        "did i tell",
        "did i mention",
        "about me",
        "who am i",
        "my profile",
        "my preferences",
        "my favorite",
        "my favourite",
        "remember when",
        "remember that",
        "recall",
        "you know about me",
        "you know that i",
        "told you",
        "mentioned",
        "we talked",
        "we discussed",
        "last time",
        "previously",
    ];
    memory_triggers.iter().any(|trigger| lowered.contains(trigger))
}
