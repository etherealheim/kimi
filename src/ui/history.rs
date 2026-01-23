use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::App;
use crate::app::PENDING_SUMMARY_LABEL;
use crate::ui::components;
use crate::ui::utils::centered_rect;

pub fn render_history_view(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // History list
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    if let [header, list, footer] = &chunks[..] {
        render_history_header(f, app, *header);
        render_history_list(f, app, *list);
        render_history_footer(f, app, *footer);
    }
    if app.history_delete_all_active {
        render_history_delete_all_modal(f, app);
    }
}

fn render_history_header(f: &mut Frame, app: &App, area: Rect) {
    let count = app.history_conversations.len();
    let count_text = if count == 0 {
        String::new()
    } else {
        format!(" ({} conversations)", count)
    };

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
            Span::styled("History", Style::default().fg(Color::Cyan)),
            Span::styled(&count_text, Style::default().fg(Color::DarkGray)),
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

fn render_history_list(f: &mut Frame, app: &App, area: Rect) {
    let mut items = Vec::new();
    let mut selectable_item_count = 0;
    let mut selected_item_index: Option<usize> = None;

    let filter_content = app.history_filter.content();
    let filter_placeholder = if filter_content.is_empty() {
        "Filter history..."
    } else {
        filter_content
    };
    let filter_style = if app.history_filter_active {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut filter_spans = vec![
        Span::styled(" ", Style::default()),
        Span::styled(" ^F ", Style::default().fg(Color::Black).bg(Color::Yellow)),
        Span::styled(" ", Style::default()),
        Span::styled(filter_placeholder, filter_style),
    ];
    if app.history_filter_active {
        filter_spans.push(Span::styled(
            "█",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::SLOW_BLINK),
        ));
    }
    items.push(ListItem::new(Line::from(filter_spans)));
    items.push(ListItem::new(Line::from("")));
    items.push(ListItem::new(Line::from("")));

    if app.history_conversations.is_empty() {
        // Better empty state with helpful message
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("No conversations yet", Style::default().fg(Color::DarkGray)),
        ])));
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(" to start a new chat", Style::default().fg(Color::DarkGray)),
        ])));
    } else {
        for (i, conv) in app.history_conversations.iter().enumerate() {
            let is_selected = i == app.history_selected_index;

            // Parse ISO date to more readable format
            let date_display =
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&conv.created_at) {
                    dt.format("%b %d, %H:%M").to_string()
                } else {
                    conv.created_at.clone()
                };

            let is_generating = is_pending_summary(app, conv);
            let summary_text = if is_generating {
                "Generating summary...".to_string()
            } else {
                conv.summary
                    .clone()
                    .unwrap_or_else(|| "Untitled conversation".to_string())
            };

            // Selection styles
            let (prefix, prefix_style) = if is_selected {
                (
                    " > ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("   ", Style::default())
            };

            let summary_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let meta_style = Style::default().fg(Color::DarkGray);

            let max_summary_width = area.width.saturating_sub(6) as usize;
            let summary_lines = wrap_summary_text(&summary_text, max_summary_width, 3);

            let first_summary_line = summary_lines.first().cloned().unwrap_or_default();
            let summary_line = Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(first_summary_line, summary_style),
            ]);

            // Second line: metadata (date, agent, message count)
            let mut meta_spans = vec![
                Span::styled("   ", meta_style),
                Span::styled(date_display, meta_style),
                Span::styled(" · ", meta_style),
                Span::styled(conv.agent_name.clone(), Style::default().fg(Color::Green)),
                Span::styled(format!(" · {} messages", conv.message_count), meta_style),
            ];
            if is_generating {
                meta_spans.push(Span::styled(" · ", meta_style));
                meta_spans.push(Span::styled(
                    PENDING_SUMMARY_LABEL,
                    Style::default().fg(Color::Yellow),
                ));
            }
            let meta_line = Line::from(meta_spans);

            let mut item_lines = vec![summary_line];
            for line in summary_lines.iter().skip(1) {
                item_lines.push(Line::from(vec![
                    Span::styled("     ", prefix_style),
                    Span::styled(line.clone(), summary_style),
                ]));
            }
            item_lines.push(meta_line);

            items.push(ListItem::new(item_lines));
            if is_selected {
                selected_item_index = Some(items.len().saturating_sub(1));
            }
            items.push(ListItem::new(Line::from("")));
            selectable_item_count += 1;
        }
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversations ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    // Calculate the position to render based on selected index
    let mut list_state = ListState::default();
    if selectable_item_count > 0
        && let Some(item_index) = selected_item_index
    {
        list_state.select(Some(item_index));
    }

    f.render_stateful_widget(list, area, &mut list_state);
}

fn is_pending_summary(app: &App, conv: &crate::storage::ConversationSummary) -> bool {
    if conv.summary.as_deref() != Some(PENDING_SUMMARY_LABEL) {
        return false;
    }
    if !app.is_generating_summary {
        return false;
    }
    let Some(current_id) = app.current_conversation_id.as_deref() else {
        return false;
    };
    normalize_conversation_id(&conv.id) == normalize_conversation_id(current_id)
}

fn normalize_conversation_id(value: &str) -> &str {
    value.strip_prefix("conversation:").unwrap_or(value)
}

fn wrap_summary_text(text: &str, max_width: usize, max_lines: usize) -> Vec<String> {
    if max_width == 0 || max_lines == 0 {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };

        if candidate.chars().count() > max_width {
            lines.push(current);
            current = word.to_string();
            if lines.len() == max_lines {
                break;
            }
        } else {
            current = candidate;
        }
    }

    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines.truncate(max_lines);
    lines
}

fn render_history_footer(f: &mut Frame, app: &App, area: Rect) {
    let keybindings: &[(&str, &str)] = if app.history_filter_active {
        &[("Type", "filter"), ("Esc", "done")]
    } else if app.history_delete_all_active {
        &[("Enter", "confirm"), ("Esc", "cancel"), ("←/→", "choose")]
    } else {
        &[
            ("Enter", "load"),
            ("Del", "delete"),
            ("/", "menu"),
            ("Esc", "new chat"),
        ]
    };

    let status: &[(&str, bool)] = if app.history_filter_active {
        &[("FILTERING", true)]
    } else {
        &[]
    };

    components::render_navigation_footer(f, area, "HISTORY", keybindings, status);
}

fn render_history_delete_all_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(45, 30, f.area());
    f.render_widget(ratatui::widgets::Clear, area);

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    "Delete all history?",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default()),
            ]))
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let Some([content_area, buttons_area]) =
        chunks.get(0..2).and_then(|s| <&[_; 2]>::try_from(s).ok())
    else {
        return;
    };

    let warning_lines = vec![
        Line::from("This will delete all saved conversations."),
        Line::from("This action cannot be undone."),
    ];
    f.render_widget(
        Paragraph::new(warning_lines).alignment(Alignment::Center),
        *content_area,
    );

    let delete_selected = app.history_delete_all_confirm_delete;
    let delete_style = if delete_selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let cancel_style = if !delete_selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let buttons = Line::from(vec![
        Span::styled("  Delete  ", delete_style),
        Span::styled("    ", Style::default()),
        Span::styled("  Cancel  ", cancel_style),
    ]);

    f.render_widget(Paragraph::new(buttons).alignment(Alignment::Center), *buttons_area);
}
