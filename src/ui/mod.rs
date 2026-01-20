mod chat;
mod components;
mod connect;
mod history;
mod menu;
mod models;
mod utils;

use crate::app::{App, AppMode};
use ratatui::Frame;

pub fn render(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Chat => chat::render_chat_view(f, app),
        AppMode::ModelSelection => models::render_model_selection(f, app),
        AppMode::Connect => {
            // Show chat view as background, then overlay connect provider selection
            chat::render_chat_view(f, app);
            connect::render_connect_providers(f, app);
        }
        AppMode::ApiKeyInput => {
            // Show chat view as background, then overlay API key input
            chat::render_chat_view(f, app);
            connect::render_api_key_input(f, app);
        }
        AppMode::History => history::render_history_view(f, app),
        _ => chat::render_chat_view(f, app), // Fallback to chat
    }

    // Overlay command menu if active
    if app.mode == AppMode::CommandMenu {
        menu::render_command_menu(f, app);
    }
}
