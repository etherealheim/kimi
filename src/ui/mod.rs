mod chat;
mod components;
mod connect;
mod help;
mod history;
mod menu;
mod models;
mod personality;
mod identity;
mod projects;
mod utils;

use crate::app::{App, AppMode};
use ratatui::Frame;

pub fn render(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Chat => chat::render_chat_view(f, app),
        AppMode::CommandMenu => chat::render_chat_view(f, app),
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
        AppMode::Help => help::render_help_view(f),
        AppMode::PersonalitySelection => personality::render_personality_view(f, app),
        AppMode::PersonalityCreate => {
            personality::render_personality_view(f, app);
            personality::render_personality_create(f, app);
        }
        AppMode::IdentityView => identity::render_identity_view(f, app),
        AppMode::ProjectList => projects::render_project_list(f, app),
        AppMode::ProjectDetail => projects::render_project_detail(f, app),
    }

    // Overlay command menu if active
    if app.mode == AppMode::CommandMenu {
        menu::render_command_menu(f, app);
    }
}
