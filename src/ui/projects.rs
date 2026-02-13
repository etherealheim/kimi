use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::app::App;
use crate::ui::components;

// ── Project List View ───────────────────────────────────────────────────────

pub fn render_project_list(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Project list
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    if let [header, content, footer] = &chunks[..] {
        render_list_header(frame, app, *header);
        render_list_content(frame, app, *content);
        render_list_footer(frame, *footer);
    }
}

fn render_list_header(frame: &mut Frame, app: &App, area: Rect) {
    let count = app.projects.len();
    let extra = vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            format!("({} projects)", count),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    components::render_view_header_with_extra(frame, area, "Projects", extra);
}

fn render_list_content(frame: &mut Frame, app: &App, area: Rect) {
    if app.projects.is_empty() {
        let message = Paragraph::new("No projects yet. The AI will suggest creating one when you discuss a topic frequently.")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(message, area);
        return;
    }

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            let is_selected = index == app.project_selected_index;
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let count_style = Style::default().fg(Color::DarkGray);
            let desc_style = Style::default().fg(Color::Cyan);

            let prefix = if is_selected { "> " } else { "  " };
            let entry_label = if project.entry_count == 1 {
                "entry"
            } else {
                "entries"
            };

            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(&project.name, name_style),
                Span::styled(
                    format!(" ({} {})", project.entry_count, entry_label),
                    count_style,
                ),
            ];

            if !project.description.is_empty() {
                spans.push(Span::styled(
                    format!(" — {}", truncate_text(&project.description, 50)),
                    desc_style,
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, area);
}

fn render_list_footer(frame: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        frame,
        area,
        "PROJECTS",
        &[("↑↓", "navigate"), ("Enter", "view"), ("Esc", "back")],
        &[],
    );
}

// ── Project Detail View ─────────────────────────────────────────────────────

pub fn render_project_detail(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Description
            Constraint::Min(0),    // Entries
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    if let [header, description, content, footer] = &chunks[..] {
        render_detail_header(frame, app, *header);
        render_detail_description(frame, app, *description);
        render_detail_entries(frame, app, *content);
        render_detail_footer(frame, *footer);
    }
}

fn render_detail_header(frame: &mut Frame, app: &App, area: Rect) {
    let project_name = app
        .current_project_name
        .as_deref()
        .unwrap_or("Unknown");
    let entry_count = app.project_entries.len();
    let entry_label = if entry_count == 1 { "entry" } else { "entries" };

    let extra = vec![
        Span::styled(" > ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            project_name.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({} {})", entry_count, entry_label),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    components::render_view_header_with_extra(frame, area, "Projects", extra);
}

fn render_detail_description(frame: &mut Frame, app: &App, area: Rect) {
    let description = app
        .current_project_description
        .as_deref()
        .unwrap_or("No description");

    let paragraph = Paragraph::new(description)
        .style(Style::default().fg(Color::Cyan))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_detail_entries(frame: &mut Frame, app: &App, area: Rect) {
    if app.project_entries.is_empty() {
        let message = Paragraph::new("No entries yet.")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from(vec![Span::styled(
                        " Entries ",
                        Style::default().fg(Color::White),
                    )]))
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(message, area);
        return;
    }

    let items: Vec<ListItem> = app
        .project_entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let is_selected = index == app.project_entry_selected_index;
            let style = if is_selected {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            let prefix = if is_selected { "> " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(entry.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled(
                " Entries ",
                Style::default().fg(Color::White),
            )]))
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, area);
}

fn render_detail_footer(frame: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        frame,
        area,
        "PROJECT",
        &[("↑↓", "navigate"), ("Esc", "back")],
        &[],
    );
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
