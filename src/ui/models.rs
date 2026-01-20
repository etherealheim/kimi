use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, ModelSource};
use crate::ui::components;

pub fn render_model_selection(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Model list
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    if let [header, list, footer] = &chunks[..] {
        render_model_header(f, *header);
        render_model_list(f, app, *list);
        render_model_footer(f, *footer);
    }
}

fn render_model_header(f: &mut Frame, area: Rect) {
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
            Span::styled("Model Selection", Style::default().fg(Color::Cyan)),
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

fn render_model_list(f: &mut Frame, app: &App, area: Rect) {
    let agent_order = ["translate", "chat"];
    let mut items = vec![ListItem::new(Line::from(""))];
    let mut flat_index = 0;
    let mut selected_list_index: Option<usize> = None;

    for agent_name in agent_order {
        // Agent section header with better visual separation
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!(" {} ", agent_name.to_uppercase()),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ])));
        items.push(ListItem::new(Line::from(""))); // Spacing after header

        if let Some(models) = app.available_models.get(agent_name) {
            let selected = app
                .selected_models
                .get(agent_name)
                .map_or(&[][..], Vec::as_slice);

            if models.is_empty() {
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "    No models available",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )])));
            } else {
                for model in models {
                    let is_selected = selected.contains(&model.name);
                    let is_current = flat_index == app.model_selection_index;

                    // Checkbox indicator
                    let checkbox = if is_selected { "[x]" } else { "[ ]" };

                    // Row styles based on selection and availability
                    let name_style = if is_current {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else if model.is_available {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let checkbox_style = if is_current {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else if is_selected {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let source_text = match model.source {
                        ModelSource::Ollama => "Ollama",
                        ModelSource::VeniceAPI => "Venice",
                    };

                    let source_style = if is_current {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else if model.is_available {
                        Style::default().fg(Color::Blue)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(
                            if is_current { " > " } else { "   " },
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(checkbox, checkbox_style),
                        Span::raw("  "),
                        Span::styled(&model.name, name_style),
                        Span::styled("  ", name_style),
                        Span::styled(source_text, source_style),
                    ])));
                    if is_current {
                        selected_list_index = Some(items.len().saturating_sub(1));
                    }
                    flat_index += 1;
                }
            }
        }

        items.push(ListItem::new(Line::from(""))); // Spacing between sections
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Available Models ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    let mut list_state = ListState::default();
    if let Some(index) = selected_list_index {
        list_state.select(Some(index));
    }

    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_model_footer(f: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        f,
        area,
        "MODELS",
        &[("Enter", "toggle"), ("↑↓", "navigate"), ("Esc", "done")],
        &[],
    );
}
