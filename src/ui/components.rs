use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const SEPARATOR: &str = "  ";

/// Configuration for text input rendering
pub struct TextInputConfig<'a> {
    pub content: &'a str,
    pub title: &'a str,
    pub title_style: Option<Style>,
    pub placeholder: Option<&'a str>,
    pub show_cursor: bool,
    pub cursor_position: Option<usize>,
}

impl<'a> TextInputConfig<'a> {
    /// Creates a new text input configuration
    pub fn new(content: &'a str, title: &'a str) -> Self {
        Self {
            content,
            title,
            title_style: None,
            placeholder: None,
            show_cursor: true,
            cursor_position: None,
        }
    }

    /// Sets the placeholder text
    pub fn with_placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Sets whether to show the cursor
    pub fn with_cursor_visible(mut self, show_cursor: bool) -> Self {
        self.show_cursor = show_cursor;
        self
    }

    /// Sets title style
    pub fn with_title_style(mut self, title_style: Style) -> Self {
        self.title_style = Some(title_style);
        self
    }

    /// Sets cursor position (character index)
    pub fn with_cursor_position(mut self, cursor_position: usize) -> Self {
        self.cursor_position = Some(cursor_position);
        self
    }
}

/// Renders a text input field with cursor indicator
pub fn render_text_input(frame: &mut Frame, area: Rect, config: TextInputConfig) {
    let cursor_indicator = if config.show_cursor { "█" } else { "" };

    // When typing starts, show content. When empty, show cursor at start position.
    let line = if config.content.is_empty() {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                cursor_indicator,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    } else {
        let inner_width = area.width.saturating_sub(2) as usize;
        let prefix_width = 2;
        let cursor_width = if config.show_cursor { 1 } else { 0 };
        let available_width = inner_width.saturating_sub(prefix_width + cursor_width).max(1);
        let cursor_index = config
            .cursor_position
            .unwrap_or_else(|| config.content.chars().count());
        let (start, end) = visible_window(config.content, cursor_index, available_width);
        let visible_content = slice_by_chars(config.content, start, end);
        let relative_cursor = cursor_index.saturating_sub(start).min(visible_content.chars().count());
        let before = slice_by_chars(&visible_content, 0, relative_cursor);
        let after = slice_by_chars(&visible_content, relative_cursor, visible_content.chars().count());

        let mut spans = vec![Span::styled("> ", Style::default().fg(Color::Cyan))];
        spans.extend(build_input_spans(&before));
        if config.show_cursor {
            spans.push(Span::styled(
                cursor_indicator,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ));
        }
        spans.extend(build_input_spans(&after));
        Line::from(spans)
    };

    let border_color = if config.content.is_empty() {
        Color::DarkGray
    } else {
        Color::Cyan
    };

    frame.render_widget(
        Paragraph::new(line).block({
            let title_style = config.title_style.unwrap_or_else(Style::default);
            let title = Line::from(vec![Span::styled(config.title, title_style)]);
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color))
        }),
        area,
    );
}

fn visible_window(content: &str, cursor: usize, width: usize) -> (usize, usize) {
    let length = content.chars().count();
    let cursor = cursor.min(length);
    if length <= width {
        return (0, length);
    }
    let mut start = cursor.saturating_sub(width.saturating_sub(1));
    if start + width > length {
        start = length.saturating_sub(width);
    }
    (start, start + width)
}

fn slice_by_chars(value: &str, start: usize, end: usize) -> String {
    value
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn build_input_spans(content: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut index = 0;
    while let Some(start_offset) = content[index..].find("[[image:") {
        let start_index = index + start_offset;
        if start_index > index {
            spans.push(Span::styled(
                content[index..start_index].to_string(),
                Style::default().fg(Color::White),
            ));
        }
        if let Some(end_offset) = content[start_index..].find("]]") {
            let end_index = start_index + end_offset + 2;
            let label = content[start_index + 8..start_index + end_offset].trim();
            let chip_text = format!(" {} ", label);
            spans.push(Span::styled(
                chip_text,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            index = end_index;
            continue;
        }
        spans.push(Span::styled(
            content[start_index..].to_string(),
            Style::default().fg(Color::White),
        ));
        return spans;
    }

    if index < content.len() {
        spans.push(Span::styled(
            content[index..].to_string(),
            Style::default().fg(Color::White),
        ));
    }
    spans
}

/// Renders a footer with mode indicator, keybindings, and status
pub fn render_navigation_footer(
    f: &mut Frame,
    area: Rect,
    mode: &str,
    keybindings: &[(&str, &str)],
    status: &[(&str, bool)],
) {
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

    for &(key, desc) in keybindings {
        spans.push(Span::raw(SEPARATOR));
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(Color::White),
        ));
    }

    for &(label, active) in status {
        spans.push(Span::raw(SEPARATOR));
        if active {
            spans.push(Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", label),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

pub fn render_status_toast(frame: &mut Frame, area: Rect, message: &str) {
    let toast = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {} ", message),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(ratatui::layout::Alignment::Right);

    frame.render_widget(toast, area);
}
