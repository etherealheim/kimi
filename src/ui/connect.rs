use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::components;

/// Render full-screen connect view with header, provider list, and footer
pub fn render_connect_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Provider list
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    if let [header, list, footer] = &chunks[..] {
        render_connect_header(frame, *header);
        render_provider_list(frame, app, *list);
        render_connect_footer(frame, *footer);
    }
}

fn render_connect_header(frame: &mut Frame, area: Rect) {
    components::render_view_header(frame, area, "Connect");
}

fn render_provider_list(frame: &mut Frame, app: &App, area: Rect) {
    let mut items = vec![ListItem::new(Line::from(""))];

    for (index, provider) in app.connect_providers.iter().enumerate() {
        let is_current = index == app.connect_selected_provider;

        let (status_text, status_style, icon) = provider_status(app, provider);
        let name_style = components::selected_name_style(is_current);

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                components::selection_prefix(is_current),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(icon, status_style),
            Span::raw("  "),
            Span::styled(provider, name_style),
            Span::styled(
                format!("  {}", status_text),
                components::selected_secondary_style(is_current, status_style),
            ),
        ])));
    }

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Providers ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

/// Returns (status_text, status_style, icon) for a given provider
fn provider_status<'a>(app: &App, provider: &str) -> (&'a str, Style, &'a str) {
    match provider {
        "ElevenLabs" if !app.connect_elevenlabs_key.is_empty() => {
            ("configured", Style::default().fg(Color::Green), "●")
        }
        "Venice AI" if !app.connect_venice_key.is_empty() => {
            ("configured", Style::default().fg(Color::Green), "●")
        }
        "Gab AI" if !app.connect_gab_key.is_empty() => {
            ("configured", Style::default().fg(Color::Green), "●")
        }
        "Brave Search" if !app.connect_brave_key.is_empty() => {
            ("configured", Style::default().fg(Color::Green), "●")
        }
        "Obsidian" if !app.connect_obsidian_vault.trim().is_empty() => {
            ("configured", Style::default().fg(Color::Green), "●")
        }
        "ElevenLabs" | "Venice AI" | "Gab AI" | "Brave Search" | "Obsidian" => {
            ("not configured", Style::default().fg(Color::DarkGray), "○")
        }
        _ => ("unknown", Style::default().fg(Color::Red), "?"),
    }
}

fn render_connect_footer(frame: &mut Frame, area: Rect) {
    components::render_navigation_footer(
        frame,
        area,
        "CONNECT",
        &[("Enter", "configure"), ("↑↓", "navigate"), ("Esc", "back")],
        &[],
    );
}

/// Render API key input modal
pub fn render_api_key_input(f: &mut Frame, app: &App) {
    let provider_name = app.connect_current_provider.as_deref().unwrap_or("Unknown");
    let title = format!("{} API Key", provider_name);
    let area = components::render_modal_frame(f, f.area(), 70, 40, &title);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let Some([input_area, help_area]) =
        chunks.get(0..2).and_then(|s| <&[_; 2]>::try_from(s).ok())
    else {
        return;
    };

    let input_value = app.connect_api_key_input.content();
    let key_len = input_value.len();
    let (display_value, title, placeholder) = if provider_name == "Obsidian" {
        (
            input_value.to_string(),
            " Vault Name ".to_string(),
            "Obsidian vault name...",
        )
    } else {
        let masked = if key_len == 0 {
            String::new()
        } else {
            let masked_value = "*".repeat(key_len.min(40));
            if key_len > 40 {
                format!("{}...", masked_value)
            } else {
                masked_value
            }
        };
        let title = if key_len > 0 {
            format!(" API Key ({} chars) ", key_len)
        } else {
            " API Key ".to_string()
        };
        (masked, title, "Paste or type your API key...")
    };

    let config = components::TextInputConfig::new(&display_value, &title)
        .with_placeholder(placeholder)
        .with_cursor_visible(true)
        .with_title_style(Style::default().fg(Color::White));
    components::render_text_input(f, *input_area, config);

    // Help text with better formatting
    let help_text = match provider_name {
        "ElevenLabs" => vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Green)),
                Span::styled(
                    "ElevenLabs",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " - Text-to-speech for Kimi responses",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Get your key: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "https://elevenlabs.io/app/settings/api-keys",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
        ],
        "Venice AI" => vec![Line::from("")],
        "Gab AI" => vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Green)),
                Span::styled(
                    "Gab AI",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " - Arya model access",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Get your key: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "https://gab.ai",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
        ],
        "Brave Search" => vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Green)),
                Span::styled(
                    "Brave Search",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " - Web search context for chat",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Get your key: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "https://api.search.brave.com/app/keys",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
        ],
        "Obsidian" => vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Green)),
                Span::styled(
                    "Obsidian",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " - Local vault for personal context (CLI)",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Enter vault name as shown in ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "obsidian vaults",
                    Style::default().fg(Color::Blue),
                ),
            ]),
        ],
        _ => vec![Line::from("")],
    };

    f.render_widget(Paragraph::new(help_text), *help_area);
}
