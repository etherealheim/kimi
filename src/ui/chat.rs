use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::components;

use crate::app::{App, MessageRole};

/// Primary chat view with header, messages, input, and footer
pub fn render_chat_view(f: &mut Frame, app: &App) {
    let has_suggestions = !app.follow_up_suggestions.is_empty();
    let suggestion_height = if has_suggestions { 3 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                     // Header
            Constraint::Min(0),                        // Chat history
            Constraint::Length(suggestion_height),      // Suggestions
            Constraint::Length(3),                      // Input
            Constraint::Length(3),                      // Footer
        ])
        .split(f.area());

    if let [header, history, suggestions, input, footer] = &chunks[..] {
        render_chat_header(f, app, *header);
        render_chat_history(f, app, *history);
        if has_suggestions {
            render_follow_up_suggestions(f, app, *suggestions);
        }
        render_chat_input(f, app, *input);
        render_chat_footer(f, app, *footer);
    }
}

fn render_chat_header(f: &mut Frame, app: &App, area: Rect) {
    // Show agent mode in title
    let agent_mode = if let Some(agent) = &app.current_agent {
        match agent.name.as_str() {
            "chat" => "Chat",
            "translate" => "Translate",
            _ => "Chat",
        }
    } else {
        "Chat"
    };

    let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let title_spans = vec![
        Span::raw(" "),
        Span::styled(
            "Kimi",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().fg(Color::DarkGray)),
        Span::styled(agent_mode, Style::default().fg(Color::Cyan)),
        Span::styled(" ", Style::default().fg(Color::DarkGray)),
        Span::styled(version_text, Style::default().fg(Color::DarkGray)),
    ];

    let model_name = app
        .current_agent
        .as_ref()
        .map_or("", |agent| agent.model.as_str());

    let border_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(border_block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let model_width = display_width(model_name) as u16 + 2;
    let left_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width.saturating_sub(model_width),
        height: inner.height,
    };
    let right_area = Rect {
        x: inner.x + inner.width.saturating_sub(model_width),
        y: inner.y,
        width: model_width,
        height: inner.height,
    };

    f.render_widget(
        Paragraph::new(Line::from(title_spans)).alignment(Alignment::Left),
        left_area,
    );
    if !model_name.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(" {} ", model_name),
                Style::default().fg(Color::White),
            )]))
            .alignment(Alignment::Right),
            right_area,
        );
    }
}

/// Styles for rendering different message types
struct MessageStyles {
    prefix: String,
    prefix_style: Style,
    content_style: Style,
    role_indicator: &'static str,
}

impl MessageStyles {
    /// Returns appropriate styles based on message role
    fn for_role(role: &MessageRole, assistant_name: Option<&str>) -> Self {
        match role {
            MessageRole::User => Self {
                prefix: "You".to_string(),
                prefix_style: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                content_style: Style::default().fg(Color::White),
                role_indicator: ">",
            },
            MessageRole::Assistant => Self {
                prefix: assistant_name.unwrap_or("Kimi").to_string(),
                prefix_style: Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
                content_style: Style::default().fg(Color::White),
                role_indicator: "<",
            },
            MessageRole::System => Self {
                prefix: String::new(),
                prefix_style: Style::default().fg(Color::DarkGray),
                content_style: Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
                role_indicator: "",
            },
        }
    }
}

/// Adds welcome message lines when chat is empty
fn add_welcome_message(lines: &mut Vec<Line>, max_width: usize) {
    let welcome_style = Style::default().fg(Color::DarkGray);
    
    // Add space for image (will be rendered separately)
    for _ in 0..18 {
        lines.push(Line::from(""));
    }
    
    // Add welcome text below image
    lines.push(Line::from(""));
    let greeting = "Hi! I'm Kimi, your helpful companion.";
    let wrapped = wrap_text(greeting, max_width, 1);
    for line in wrapped {
        lines.push(Line::from(vec![
            Span::styled("  ", welcome_style),
            Span::styled(line, welcome_style),
        ]));
    }
    
    lines.push(Line::from(""));
    let prompt = "What is on your mind today?";
    let prompt_style = Style::default().fg(Color::Cyan);
    lines.push(Line::from(vec![
        Span::styled("  ", welcome_style),
        Span::styled(prompt, prompt_style),
    ]));
    lines.push(Line::from(""));
}

/// Renders a system message (compact, subtle styling)
fn render_system_message(
    message: &crate::app::ChatMessage,
    content_style: Style,
    max_content_width: usize,
) -> Vec<Line<'static>> {
    let wrapped = wrap_text(&message.content, max_content_width, 1);
    wrapped
        .into_iter()
        .map(|line| {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(line, content_style),
            ])
        })
        .collect()
}

/// Renders a user or assistant message with header and content
fn render_regular_message(
    message: &crate::app::ChatMessage,
    styles: &MessageStyles,
    max_content_width: usize,
) -> Vec<Line<'static>> {
    let mut message_lines = Vec::new();

    // Message header with role indicator
    let header_spans = vec![
        Span::styled(
            format!(" {} ", styles.role_indicator),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(styles.prefix.clone(), styles.prefix_style),
        Span::styled(
            format!("  {}", message.timestamp),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    // Context usage info removed - cleaner UI
    message_lines.push(Line::from(header_spans));

    // Message content with proper indentation
    let max_empty_lines = 1;
    let wrapped_content = wrap_text(&message.content, max_content_width, max_empty_lines);
    for content_line in wrapped_content {
        message_lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(content_line, styles.content_style),
        ]));
    }
    message_lines
}

/// Adds loading indicator animation
fn add_loading_indicator(
    lines: &mut Vec<Line>,
    app: &App,
    label: &str,
    frame: u8,
    suffix: Option<String>,
) {
    let dots_frames = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    let frame_index = (frame as usize) % dots_frames.len();
    let dots = dots_frames.get(frame_index).copied().unwrap_or("⣷").to_string();
    let assistant_name = if app.personality_enabled {
        app.personality_name.as_deref().unwrap_or("Kimi")
    } else {
        "Kimi"
    };

    let name_chars: Vec<char> = assistant_name.chars().collect();
    let pulse_index = pulse_index_for_frame(frame, name_chars.len());
    let mut kimi_spans = Vec::new();
    for (char_index, character) in name_chars.iter().copied().enumerate() {
        let is_bright = pulse_index == Some(char_index);
        let kimi_style = if is_bright {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::DIM)
        };
        kimi_spans.push(Span::styled(character.to_string(), kimi_style));
    }

    let mut line_spans = vec![Span::styled(" < ", Style::default().fg(Color::DarkGray))];
    line_spans.extend(kimi_spans);
    let mut status = format!(" {}", label);
    if let Some(suffix) = suffix {
        status = format!("{} {}", status, suffix);
    }
    status = format!("{} {}", status, dots);
    line_spans.extend(vec![Span::styled(
        status,
        Style::default().fg(Color::DarkGray),
    )]);
    lines.push(Line::from(line_spans));
}

fn pulse_index_for_frame(frame: u8, name_len: usize) -> Option<usize> {
    if name_len == 0 {
        return None;
    }

    let max_index = name_len.saturating_sub(1);
    if max_index == 0 {
        return Some(0);
    }

    let cycle_len = max_index * 2;
    let frame_index = (frame as usize) % cycle_len;
    let pulse_index = if frame_index <= max_index {
        frame_index
    } else {
        cycle_len - frame_index
    };
    Some(pulse_index)
}

/// Calculates scroll position based on viewport and offset
fn calculate_scroll_position(
    total_lines: usize,
    visible_height: usize,
    chat_scroll_offset: usize,
    chat_auto_scroll: bool,
) -> (usize, usize) {
    let max_scroll_offset = total_lines.saturating_sub(visible_height);
    let actual_scroll_offset = chat_scroll_offset.min(max_scroll_offset);

    let scroll_from_top = if total_lines <= visible_height {
        0
    } else if chat_auto_scroll && actual_scroll_offset == 0 {
        max_scroll_offset
    } else {
        max_scroll_offset.saturating_sub(actual_scroll_offset)
    };

    (scroll_from_top, actual_scroll_offset)
}

fn render_chat_history(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let content_width = area.width.saturating_sub(2) as usize;
    let max_content_width = content_width.saturating_sub(6).max(1);
    let max_system_width = content_width.saturating_sub(4).max(1);
    let visible_height = area.height.saturating_sub(2) as usize;

    // Welcome message if chat is empty
    if app.chat_history.is_empty() && !app.is_loading {
        add_welcome_message(&mut lines, max_content_width);
        let total_with_padding = lines.len().saturating_add(1);
        if total_with_padding < visible_height {
            let pad_count = visible_height - total_with_padding;
            let mut padding: Vec<Line> =
                (0..pad_count).map(|_| Line::from("")).collect();
            padding.append(&mut lines);
            lines = padding;
        }
    }

    // Build all message lines
    for message in &app.chat_history {
        let assistant_name = message.display_name.as_deref();
        let styles = MessageStyles::for_role(&message.role, assistant_name);

        add_spacing(&mut lines, 1);

        if message.role == MessageRole::User {
            add_spacing(&mut lines, 1);
        }

        if message.role == MessageRole::System {
            lines.extend(render_system_message(
                message,
                styles.content_style,
                max_system_width,
            ));
        } else {
            lines.extend(render_regular_message(
                message,
                &styles,
                max_content_width,
            ));
        }
    }

    // Add loading indicator if processing
    if app.is_loading {
        add_spacing(&mut lines, 1);
        // Use real-time activity status if available, otherwise fallback to heuristics
        let loading_label = if let Some(activity) = &app.current_activity {
            activity.as_str()
        } else if app.is_searching {
            "searching"
        } else if app.is_fetching_notes {
            "fetching"
        } else {
            "thinking"
        };
        add_loading_indicator(&mut lines, app, loading_label, app.loading_frame, None);
    }

    // Display each active download
    for download in &app.active_downloads {
        add_spacing(&mut lines, 1);
        let progress = download.progress.map(|value| format!("{}%", value));

        // Truncate URL for display if too long (char-safe to avoid UTF-8 panic)
        let display_url = if download.url.chars().count() > 50 {
            let truncated: String = download.url.chars().take(47).collect();
            format!("{}...", truncated)
        } else {
            download.url.clone()
        };

        let label = format!("downloading {}", display_url);
        add_loading_indicator(&mut lines, app, &label, download.frame, progress);
    }

    if app.conversion_active {
        add_spacing(&mut lines, 1);
        add_loading_indicator(&mut lines, app, "converting", app.conversion_frame, None);
    }

    if app.summary_active {
        add_spacing(&mut lines, 1);
        add_loading_indicator(&mut lines, app, "summarizing", app.summary_frame, None);
    }

    // Bottom padding
    add_spacing(&mut lines, 1);

    // Calculate viewport and scroll position
    let total_lines = lines.len();
    let (scroll_from_top, actual_scroll_offset) = calculate_scroll_position(
        total_lines,
        visible_height,
        app.chat_scroll_offset,
        app.chat_auto_scroll,
    );

    // Build title with compact scroll indicator
    let title_spans = if actual_scroll_offset > 0 {
        vec![
            Span::styled(" Conversation ", Style::default().fg(Color::White)),
            Span::styled(
                format!("[+{} lines] ", actual_scroll_offset),
                Style::default().fg(Color::Yellow),
            ),
        ]
    } else {
        vec![Span::styled(
            " Conversation ",
            Style::default().fg(Color::White),
        )]
    };

    let content = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(title_spans))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .scroll((scroll_from_top as u16, 0));

    frame.render_widget(content, area);
}

fn wrap_text(text: &str, max_width: usize, max_empty_lines: usize) -> Vec<String> {
    let mut lines = wrap_text_impl(text, max_width);
    trim_empty_edges(&mut lines);
    collapse_empty_lines(&mut lines, max_empty_lines);
    lines
}

fn wrap_text_impl(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let characters: Vec<char> = raw_line.chars().collect();
        let mut start = 0usize;
        let mut index = 0usize;
        let mut width = 0usize;
        let mut last_space: Option<usize> = None;

        while index < characters.len() {
            let Some(&character) = characters.get(index) else {
                break;
            };
            let char_width = UnicodeWidthChar::width(character).unwrap_or(0).max(1);

            if character.is_whitespace() {
                last_space = Some(index);
            }

            if width + char_width > max_width && width > 0 {
                let end = last_space.filter(|space| *space > start).unwrap_or(index);
                let line: String = characters
                    .get(start..end)
                    .map(|slice| slice.iter().collect())
                    .unwrap_or_default();
                lines.push(line.trim_end().to_string());

                start = if end < characters.len()
                    && characters.get(end).is_some_and(|c| c.is_whitespace())
                {
                    end + 1
                } else {
                    end
                };
                index = start;
                width = 0;
                last_space = None;
                continue;
            }

            width += char_width;
            index += 1;
        }

        if start < characters.len() {
            let line: String = characters
                .get(start..)
                .map(|slice| slice.iter().collect())
                .unwrap_or_default();
            lines.push(line.trim_end().to_string());
        }
    }
    lines
}

fn trim_empty_edges(lines: &mut Vec<String>) {
    while lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
}

fn collapse_empty_lines(lines: &mut Vec<String>, max_empty_lines: usize) {
    if max_empty_lines == 0 {
        lines.retain(|line| !line.is_empty());
        return;
    }

    let mut result = Vec::with_capacity(lines.len());
    let mut empty_run = 0usize;
    for line in lines.iter() {
        if line.is_empty() {
            empty_run += 1;
            if empty_run <= max_empty_lines {
                result.push(String::new());
            }
        } else {
            empty_run = 0;
            result.push(line.clone());
        }
    }
    *lines = result;
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn add_spacing(lines: &mut Vec<Line>, count: usize) {
    let mut remaining = count;
    while remaining > 0 {
        if lines.last().is_none_or(|line| !line.to_string().is_empty()) {
            lines.push(Line::from(""));
        }
        remaining -= 1;
    }
}

/// Renders follow-up suggestion pills as selectable options
fn render_follow_up_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    let suggestions = &app.follow_up_suggestions;
    if suggestions.is_empty() {
        return;
    }

    let inner_width = area.width.saturating_sub(4) as usize; // border + padding
    let pill_count = suggestions.len();
    let separator_width = 3; // " │ " between pills
    let total_separator = separator_width * pill_count.saturating_sub(1);
    let pill_text_budget = inner_width.saturating_sub(total_separator) / pill_count.max(1);

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(" "));

    for (index, suggestion) in suggestions.iter().enumerate() {
        let is_selected = app.suggestion_mode_active && app.suggestion_selected_index == index;

        // Truncate text to fit budget
        let display_text: String = if suggestion.chars().count() > pill_text_budget {
            let truncated: String = suggestion
                .chars()
                .take(pill_text_budget.saturating_sub(1))
                .collect();
            format!("{}…", truncated)
        } else {
            suggestion.clone()
        };

        let text_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        spans.push(Span::styled(format!(" {} ", display_text), text_style));

        // Add separator between pills
        if index < pill_count.saturating_sub(1) {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let paragraph = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}

fn render_chat_input(frame: &mut Frame, app: &App, area: Rect) {
    let placeholder_buffer;
    let placeholder_text = if app.is_loading {
        // Use real-time activity status if available, otherwise fallback to heuristics
        if let Some(activity) = &app.current_activity {
            // Capitalize first letter for placeholder text
            if let Some(first_char) = activity.chars().next() {
                placeholder_buffer = format!(
                    "{}{}...",
                    first_char.to_uppercase(),
                    &activity[first_char.len_utf8()..]
                );
                placeholder_buffer.as_str()
            } else {
                "Processing..."
            }
        } else if app.is_searching {
            "Searching..."
        } else if app.is_fetching_notes {
            "Fetching notes..."
        } else {
            "Thinking..."
        }
    } else {
        "Type your message here..."
    };

    let config = components::TextInputConfig::new(app.chat_input.content(), " Message ")
        .with_placeholder(placeholder_text)
        .with_cursor_visible(!app.is_loading)
        .with_title_style(Style::default().fg(Color::White))
        .with_cursor_position(app.chat_input.cursor_position());

    components::render_text_input(frame, area, config);
}

fn render_chat_footer(f: &mut Frame, app: &App, area: Rect) {
    let keybindings = [("/", "menu"), ("Tab", "switch"), ("^R", "speak"), ("Esc", "history")];

    let border_block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray));
    f.render_widget(border_block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let toast_message = app.status_toast_message();
    let toast_width = toast_message.map_or(0, |message| message.chars().count() as u16 + 4);

    let left_width = inner
        .width
        .saturating_sub(toast_width.saturating_add(1));

    let left_area = Rect {
        x: inner.x,
        y: inner.y,
        width: left_width,
        height: inner.height,
    };

    let menu_enabled = app.chat_input.is_empty();
    let keybinding_spans =
        build_footer_spans("CHAT", &keybindings, app.personality_enabled, menu_enabled);
    f.render_widget(
        Paragraph::new(Line::from(keybinding_spans)),
        left_area,
    );

    if let Some(message) = toast_message {
        let toast_area = Rect {
            x: inner.x + inner.width.saturating_sub(toast_width),
            y: inner.y,
            width: toast_width,
            height: inner.height,
        };
        components::render_status_toast(f, toast_area, message);
    }
}

fn build_footer_spans(
    mode: &str,
    keybindings: &[(&str, &str)],
    personality_enabled: bool,
    menu_enabled: bool,
) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", mode),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if personality_enabled {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            " PERSONALITY ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }

    for &(key, desc) in keybindings {
        let is_menu_key = key == "/";
        let is_disabled = is_menu_key && !menu_enabled;
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" {} ", key),
            if is_disabled {
                Style::default().fg(Color::Black).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            },
        ));
        spans.push(Span::styled(
            format!(" {}", desc),
            if is_disabled {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            },
        ));
    }

    spans
}
