use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::app::App;
use crate::ui::components;
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
    let area = components::render_modal_frame(f, f.area(), 60, 30, "New Personality");

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
    components::render_view_header(f, area, "Personalities");
}

fn render_personality_list(f: &mut Frame, app: &App, area: Rect) {
    let mut items = vec![ListItem::new(Line::from(""))];
    let mut selected_list_index: Option<usize> = None;
    let base_personality_name = crate::services::personality::base_personality_name();
    let my_personality_name = crate::services::personality::my_personality_name();
    
    // Calculate centered padding (60% width content, centered)
    let content_width = (area.width as f32 * 0.6) as u16;
    let left_padding = ((area.width.saturating_sub(content_width)) / 2) as usize;
    let padding = " ".repeat(left_padding.saturating_sub(2)); // -2 for border

    let is_base_selected = app.personality_selected_index == 0;
    items.push(ListItem::new(Line::from(vec![
        Span::raw(padding.clone()),
        Span::styled(
            components::selection_prefix(is_base_selected),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(base_personality_name, components::selected_name_style(is_base_selected)),
    ])));
    if is_base_selected {
        selected_list_index = Some(items.len().saturating_sub(1));
    }
    let is_my_selected = app.personality_selected_index == 1;
    items.push(ListItem::new(Line::from(vec![
        Span::raw(padding.clone()),
        Span::styled(
            components::selection_prefix(is_my_selected),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(my_personality_name, components::selected_name_style(is_my_selected)),
    ])));
    if is_my_selected {
        selected_list_index = Some(items.len().saturating_sub(1));
    }
    items.push(ListItem::new(Line::from("")));

    for (index, name) in app.personality_items.iter().enumerate() {
        let list_index = index + 2;
        let is_selected = list_index == app.personality_selected_index;
        let is_active = app.personality_name.as_deref() == Some(name.as_str());

        let name_style = components::selected_name_style(is_selected);

        let checkbox_style = if is_selected {
            components::selected_secondary_style(true, Style::default())
        } else if is_active {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let checkbox = if is_active { "[x]" } else { "[ ]" };

        items.push(ListItem::new(Line::from(vec![
            Span::raw(padding.clone()),
            Span::styled(
                components::selection_prefix(is_selected),
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
        "PERSONALITIES",
        &[
            ("Enter", "open"),
            ("N", "new"),
            ("E", "edit"),
            ("Del", "delete"),
            ("Esc", "back"),
        ],
        &[],
    );
}
