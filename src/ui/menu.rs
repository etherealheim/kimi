use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::components;

pub fn render_command_menu(frame: &mut Frame, app: &App) {
    let filtered_items = app.filtered_items();

    let area = frame.area();
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Search
            Constraint::Min(0),    // List
        ])
        .split(area);

    if let [header_area, search_area, list_area] = &chunks[..] {
        render_command_header(frame, *header_area);
        render_search_input(frame, app, *search_area);

        if filtered_items.is_empty() && !app.input.is_empty() {
            render_empty_message(frame, *list_area);
        } else if !filtered_items.is_empty() {
            render_command_list(frame, app, &filtered_items, *list_area);
        }
    }
}

fn render_command_header(frame: &mut Frame, area: Rect) {
    components::render_view_header(frame, area, "Commands");
}

fn render_search_input(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = if app.input.is_empty() {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "type to filter",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(&app.input, Style::default().fg(Color::White)),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    let search_input = Paragraph::new(prompt).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(search_input, area);
}

fn render_empty_message(frame: &mut Frame, area: Rect) {
    let empty_msg = Paragraph::new(Line::from(vec![Span::styled(
        "No matching commands",
        Style::default().fg(Color::DarkGray),
    )]))
    .alignment(Alignment::Left);
    frame.render_widget(empty_msg, area);
}

fn render_command_list(
    frame: &mut Frame,
    app: &App,
    filtered_items: &[crate::app::MenuItem],
    area: Rect,
) {
    let items: Vec<ListItem> = filtered_items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let is_selected = index == app.selected_index;
            let prefix = if is_selected { "> " } else { "  " };
            let name_style = components::selected_name_style(is_selected);
            let description_style =
                components::selected_secondary_style(is_selected, Style::default().fg(Color::DarkGray));

            ListItem::new(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                Span::styled(" ", Style::default()),
                Span::styled(&item.name, name_style),
                Span::styled("  —  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&item.description, description_style),
            ]))
        })
        .collect();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

