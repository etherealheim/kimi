use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::App;
use crate::ui::components;
use crate::ui::utils::centered_rect;

pub fn render_personality_view(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // List
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    if let [header, list, footer] = &chunks[..] {
        render_personality_header(f, *header);
        render_personality_list(f, app, *list);
        render_personality_footer(f, *footer);
    }
}

pub fn render_personality_create(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    "New Personality",
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
        .constraints([Constraint::Length(3)])
        .split(area);

    let Some([input_area]) = chunks.get(0..1).and_then(|s| <&[_; 1]>::try_from(s).ok()) else {
        return;
    };

    let config = components::TextInputConfig::new(
        app.personality_create_input.content(),
        " Name ",
    )
    .with_placeholder("Type a personality name...");
    components::render_text_input(f, *input_area, config);

}

fn render_personality_header(f: &mut Frame, area: Rect) {
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
            Span::styled("Personality", Style::default().fg(Color::Cyan)),
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

fn render_personality_list(f: &mut Frame, app: &App, area: Rect) {
    let mut items = vec![ListItem::new(Line::from(""))];
    let mut selected_list_index: Option<usize> = None;
    let my_personality_name = crate::services::personality::my_personality_name();

    let is_my_selected = app.personality_selected_index == 0;
    let my_style = if is_my_selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if is_my_selected { " > " } else { "   " },
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(my_personality_name, my_style),
    ])));
    if is_my_selected {
        selected_list_index = Some(items.len().saturating_sub(1));
    }
    items.push(ListItem::new(Line::from("")));

    for (index, name) in app.personality_items.iter().enumerate() {
        let list_index = index + 1;
        let is_selected = list_index == app.personality_selected_index;
        let is_active = app.personality_name.as_deref() == Some(name.as_str());

        let name_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let checkbox_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else if is_active {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let checkbox = if is_active { "[x]" } else { "[ ]" };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                if is_selected { " > " } else { "   " },
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(checkbox, checkbox_style),
            Span::raw("  "),
            Span::styled(name, name_style),
        ])));

        if is_selected {
            selected_list_index = Some(items.len().saturating_sub(1));
        }
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Personalities ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    let mut list_state = ListState::default();
    if let Some(index) = selected_list_index {
        list_state.select(Some(index));
    }

    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_personality_footer(f: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        f,
        area,
        "PERSONALITY",
        &[
            ("Enter", "select"),
            ("N", "new"),
            ("E", "edit"),
            ("Del", "delete"),
            ("Esc", "back"),
        ],
        &[],
    );
}
