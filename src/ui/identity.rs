use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::services::identity::{DreamEntry, IdentityState, IdentityTrait};
use crate::ui::components;

pub fn render_identity_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    if let [header, input, content, footer] = &chunks[..] {
        render_header(frame, *header);
        render_core_input(frame, app, *input);
        render_identity_columns(frame, *content);
        render_footer(frame, *footer);
    }
}

fn render_header(frame: &mut Frame, area: Rect) {
    components::render_view_header(frame, area, "Identity");
}

fn render_core_input(frame: &mut Frame, _app: &App, area: Rect) {
    // Read core belief from state
    let core_belief = crate::services::identity::read_primary_core_belief()
        .unwrap_or_else(|_| "No core belief set.".to_string());
    
    let paragraph = Paragraph::new(core_belief)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![Span::styled(
                    " Core belief ",
                    Style::default().fg(Color::White),
                )]))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left);
    
    frame.render_widget(paragraph, area);
}

fn render_identity_columns(frame: &mut Frame, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    if let [traits_area, dreams_area] = &columns[..] {
        let state = crate::services::identity::read_identity_state().ok();
        render_traits_panel(frame, *traits_area, state.as_ref());
        render_dreams_panel(frame, *dreams_area, state.as_ref());
    }
}

fn render_traits_panel(frame: &mut Frame, area: Rect, state: Option<&IdentityState>) {
    let empty_traits: &[IdentityTrait] = &[];
    let traits = state.map_or(empty_traits, |value| value.traits.as_slice());
    
    let trait_count = traits.len();
    
    let mut items = vec![ListItem::new(Line::from(""))]; // Empty space at top
    if traits.is_empty() {
        items.push(ListItem::new(Line::from("No traits yet.")));
    } else {
        items.extend(traits.iter().map(trait_list_item));
    }
    
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" Traits ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("({}) ", trait_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, area);
}

fn trait_list_item(entry: &IdentityTrait) -> ListItem<'_> {
    let sign = if entry.strength >= 0.0 { "+" } else { "" };
    let color = if entry.strength.abs() > 0.7 {
        Color::Yellow // Strong traits
    } else if entry.strength.abs() > 0.3 {
        Color::Cyan // Moderate traits
    } else {
        Color::DarkGray // Weak/neutral traits
    };
    
    ListItem::new(Line::from(vec![
        Span::styled(
            entry.name.clone(),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}{:.1}", sign, entry.strength),
            Style::default().fg(color),
        ),
    ]))
}

fn render_dreams_panel(frame: &mut Frame, area: Rect, state: Option<&IdentityState>) {
    let dreams = state.map(|value| &value.dreams);
    let (active_count, backlog_count) = dreams.map_or((0, 0), |d| (d.active.len(), d.backlog.len()));
    let max_active = 3;
    let max_backlog = 5;
    
    let mut items = vec![ListItem::new(Line::from(""))]; // Empty space at top
    if let Some(dreams) = dreams {
        if !dreams.active.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("Active", Style::default().fg(Color::Magenta)),
            ])));
            items.extend(dreams.active.iter().map(|entry| dream_list_item(entry, true)));
            items.push(ListItem::new(Line::from("")));
        }
        if !dreams.backlog.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("Backlog", Style::default().fg(Color::DarkGray)),
            ])));
            items.extend(dreams.backlog.iter().map(|entry| dream_list_item(entry, false)));
        }
    }
    if items.len() == 1 { // Only the empty space
        items.push(ListItem::new(Line::from("No dreams yet.")));
    }
    
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" Dreams ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("(A:{}/{} B:{}/{}) ", active_count, max_active, backlog_count, max_backlog),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, area);
}

fn dream_list_item(entry: &DreamEntry, is_active: bool) -> ListItem<'_> {
    let label_style = if is_active {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::White)
    };
    ListItem::new(Line::from(vec![
        Span::styled(entry.title.clone(), label_style),
        Span::raw("  "),
        Span::styled(format!("p{}", entry.priority.max(1)), Style::default().fg(Color::Cyan)),
    ]))
}

fn render_footer(frame: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        frame,
        area,
        "IDENTITY",
        &[("Esc", "back")],
        &[],
    );
}
