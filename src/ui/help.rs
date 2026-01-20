use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::ui::components;

pub fn render_help_view(f: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Body
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    if let [header, body, footer] = &chunks[..] {
        render_help_header(f, *header);
        render_help_body(f, *body);
        render_help_footer(f, *footer);
    }
}

fn render_help_header(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Kimi",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default().fg(Color::DarkGray)),
            Span::styled("Help", Style::default().fg(Color::Cyan)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Left),
        area,
    );
}

fn render_help_body(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Global shortcuts", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::styled("  Quit", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  /", Style::default().fg(Color::Yellow)),
            Span::styled("       Command menu", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Tab", Style::default().fg(Color::Yellow)),
            Span::styled("     Rotate agent", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+R", Style::default().fg(Color::Yellow)),
            Span::styled("  Speak last response", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+T", Style::default().fg(Color::Yellow)),
            Span::styled("  Toggle auto-TTS", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+P", Style::default().fg(Color::Yellow)),
            Span::styled("  Toggle personality", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+S", Style::default().fg(Color::Yellow)),
            Span::styled("  Toggle Brave search", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Esc", Style::default().fg(Color::Yellow)),
            Span::styled("     Back/close", Style::default().fg(Color::White)),
        ]),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Shortcuts ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn render_help_footer(f: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        f,
        area,
        "HELP",
        &[("Esc", "back")],
        &[],
    );
}
