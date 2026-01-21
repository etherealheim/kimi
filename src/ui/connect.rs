use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::components;
use crate::ui::utils::centered_rect;
use std::path::Path;

/// Render provider selection modal
pub fn render_connect_providers(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area());
    f.render_widget(Clear, area);

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    "API Providers",
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
        .constraints([Constraint::Min(0)])
        .split(area);

    let Some([list_area]) = chunks.get(0..1).and_then(|s| <&[_; 1]>::try_from(s).ok()) else {
        return;
    };

    let items: Vec<ListItem> = app
        .connect_providers
        .iter()
        .enumerate()
        .map(|(i, provider)| {
            let selected = i == app.connect_selected_provider;

            let (status_text, status_style, icon) = match provider.as_str() {
                "ElevenLabs" if !app.connect_elevenlabs_key.is_empty() => {
                    ("configured", Style::default().fg(Color::Green), "●")
                }
                "Venice AI" if !app.connect_venice_key.is_empty() => {
                    ("configured", Style::default().fg(Color::Green), "●")
                }
                "Brave Search" if !app.connect_brave_key.is_empty() => {
                    ("configured", Style::default().fg(Color::Green), "●")
                }
                "Obsidian" if !app.connect_obsidian_vault.trim().is_empty() => {
                    let is_valid = Path::new(&app.connect_obsidian_vault).is_dir();
                    let status_style = if is_valid {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Yellow)
                    };
                    let status_text = if is_valid {
                        "configured"
                    } else {
                        "invalid path"
                    };
                    (status_text, status_style, "●")
                }
                "ElevenLabs" | "Venice AI" | "Brave Search" => {
                    ("not configured", Style::default().fg(Color::DarkGray), "○")
                }
                "Obsidian" => ("not configured", Style::default().fg(Color::DarkGray), "○"),
                _ => ("unknown", Style::default().fg(Color::Red), "?"),
            };

            let name_style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { " > " } else { "   " },
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(icon, status_style),
                Span::raw("  "),
                Span::styled(provider, name_style),
                Span::styled(
                    format!("  {}", status_text),
                    if selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        status_style
                    },
                ),
            ]))
        })
        .collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Select Provider ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        *list_area,
    );
}

/// Render API key input modal
pub fn render_api_key_input(f: &mut Frame, app: &App) {
    let area = centered_rect(70, 40, f.area());
    f.render_widget(Clear, area);

    let provider_name = app.connect_current_provider.as_deref().unwrap_or("Unknown");

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!("{} API Key", provider_name),
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
            " Vault Path ".to_string(),
            "Path to your Obsidian vault...",
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
                    " - Local vault for personal context",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Example: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "/Users/you/Documents/Obsidian",
                    Style::default().fg(Color::Blue),
                ),
            ]),
        ],
        _ => vec![Line::from("")],
    };

    f.render_widget(Paragraph::new(help_text), *help_area);
}
