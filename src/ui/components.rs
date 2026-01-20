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
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(config.content, Style::default().fg(Color::White)),
            Span::styled(
                cursor_indicator,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
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
