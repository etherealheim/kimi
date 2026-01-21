// Defensive programming lints - prevent panics and unsafe patterns
#![deny(clippy::indexing_slicing)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::wildcard_enum_match_arm)]
#![warn(clippy::fn_params_excessive_bools)]
// Idiomatic Rust lints
#![warn(clippy::needless_return)]
#![warn(clippy::let_and_return)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::redundant_closure_for_method_calls)]
#![warn(clippy::map_unwrap_or)]
#![warn(clippy::explicit_iter_loop)]

mod agents;
mod app;
mod config;
mod services;
mod storage;
mod ui;

use app::{App, AppMode, Navigable};
use color_eyre::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
};
use services::weather::WeatherService;
use std::{io, time::Duration};

fn main() -> Result<()> {
    // Setup error handling
    color_eyre::install()?;

    // Load config
    let config = config::Config::load()?;

    // Check for command-line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return handle_cli_args(&args);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and initialize services
    let mut app = App::new();
    app.init_services(&config);
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn handle_cli_args(args: &[String]) -> Result<()> {
    let cmd = args
        .get(1)
        .ok_or_else(|| color_eyre::eyre::eyre!("No command provided"))?;
    let program_name = args.first().map_or("kimi", String::as_str);

    match cmd.as_str() {
        "--help" | "-h" => print_help(program_name),
        "--version" | "-v" => println!("Kimi The Rust CLI v0.1.0"),
        "weather" => {
            let weather_service = WeatherService::new();
            let weather_json = weather_service.fetch_current_weather_json()?;
            println!("{}", weather_json);
        }
        "personality" => {
            let config = config::Config::load()?;
            let selected = if config.personality.selected.is_empty() {
                services::personality::default_personality_name()
            } else {
                config.personality.selected
            };
            if services::personality::open_personality_in_new_terminal(&selected).is_err() {
                services::personality::open_personality_in_place(&selected)?;
            }
        }
        cmd_str => {
            let mut app = App::new();
            if app.command_handlers.contains_key(cmd_str) {
                app.execute_command(cmd_str)?;
                if let Some(msg) = app.messages.last() {
                    println!("{}", msg);
                }
            } else {
                eprintln!("Unknown command: {}", cmd_str);
                eprintln!("Run with --help for available commands.");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

fn print_help(program_name: &str) {
    println!("Kimi The Rust CLI - AI Agent Toolkit");
    println!();
    println!("Usage: {} [command]", program_name);
    println!();
    println!("Commands:");
    println!("  weather    - Print Prague weather JSON");
    println!("  personality - Edit system personality in micro");
    println!("  help       - Show help information");
    println!("  --help     - Show this help");
    println!("  --version  - Show version");
    println!();
    println!("Run without arguments to start interactive mode.");
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        // Check for agent responses
        app.check_agent_response();
        tick_loading_animation(app);
        tick_download_animation(app);
        tick_conversion_animation(app);
        tick_summary_animation(app);
        app.clear_expired_status_toast();

        terminal.draw(|f| ui::render(f, app))?;

        if app.should_quit {
            break;
        }

        // Poll for events with a timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    // Only handle KeyPress events to avoid duplicate handling
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        app.should_quit = true;
                        continue;
                    }

                    if matches!(key.code, KeyCode::Char('/'))
                        && key.modifiers == KeyModifiers::NONE
                        && app.mode == AppMode::Chat
                        && app.chat_input.is_empty()
                    {
                        app.open_command_menu();
                        continue;
                    }

                    match app.mode {
                        AppMode::CommandMenu => handle_command_menu(app, key.code)?,
                        AppMode::Chat => handle_chat_mode(app, key.code, key.modifiers)?,
                        AppMode::ModelSelection => handle_model_selection(app, key.code)?,
                        AppMode::Connect => handle_connect_mode(app, key.code)?,
                        AppMode::ApiKeyInput => handle_api_key_input_mode(app, key.code)?,
                        AppMode::History => handle_history_mode(app, key.code, key.modifiers)?,
                        AppMode::Help => handle_help_mode(app, key.code)?,
                        AppMode::PersonalitySelection => {
                            handle_personality_selection_mode(app, key.code)?
                        }
                        AppMode::PersonalityCreate => {
                            handle_personality_create_mode(app, key.code)?
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse_event(app, mouse)?;
                }
                Event::Paste(paste) => {
                    handle_paste(app, &paste)?;
                }
                Event::FocusGained | Event::FocusLost | Event::Resize(_, _) => {}
            }
        }
    }

    Ok(())
}

fn tick_loading_animation(app: &mut App) {
    use std::time::{Duration, Instant};
    if !app.is_loading {
        app.loading_frame = 0;
        app.last_loading_tick = None;
        return;
    }

    let now = Instant::now();
    let should_tick = app
        .last_loading_tick
        .map(|last_tick| now.duration_since(last_tick) >= Duration::from_millis(200))
        .unwrap_or(true);

    if should_tick {
        app.loading_frame = app.loading_frame.wrapping_add(1);
        app.last_loading_tick = Some(now);
    }
}

fn tick_download_animation(app: &mut App) {
    use std::time::{Duration, Instant};
    if !app.download_active {
        app.download_frame = 0;
        app.last_download_tick = None;
        return;
    }

    let now = Instant::now();
    let should_tick = app
        .last_download_tick
        .map(|last_tick| now.duration_since(last_tick) >= Duration::from_millis(200))
        .unwrap_or(true);

    if should_tick {
        app.download_frame = app.download_frame.wrapping_add(1);
        app.last_download_tick = Some(now);
    }
}

fn tick_conversion_animation(app: &mut App) {
    use std::time::{Duration, Instant};
    if !app.conversion_active {
        app.conversion_frame = 0;
        app.last_conversion_tick = None;
        return;
    }

    let now = Instant::now();
    let should_tick = app
        .last_conversion_tick
        .map(|last_tick| now.duration_since(last_tick) >= Duration::from_millis(200))
        .unwrap_or(true);

    if should_tick {
        app.conversion_frame = app.conversion_frame.wrapping_add(1);
        app.last_conversion_tick = Some(now);
    }
}

fn tick_summary_animation(app: &mut App) {
    use std::time::{Duration, Instant};
    if !app.summary_active {
        app.summary_frame = 0;
        app.last_summary_tick = None;
        return;
    }

    let now = Instant::now();
    let should_tick = app
        .last_summary_tick
        .map(|last_tick| now.duration_since(last_tick) >= Duration::from_millis(200))
        .unwrap_or(true);

    if should_tick {
        app.summary_frame = app.summary_frame.wrapping_add(1);
        app.last_summary_tick = Some(now);
    }
}

fn handle_command_menu(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_menu(),
        KeyCode::Enter => {
            app.execute_selected()?;
            // Don't call close_menu() here - let the command itself determine the mode
        }
        KeyCode::Up => app.previous_item(),
        KeyCode::Down => app.next_item(),
        KeyCode::Char(character) => {
            app.add_input_char(character);
            let input_snapshot = app.input.clone();
            if app.try_add_image_attachment_from_text(input_snapshot.as_str())? {
                app.show_status_toast("IMAGE ADDED");
                app.close_menu();
            }
        }
        KeyCode::Backspace => app.remove_input_char(),
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_model_selection(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_model_selection(),
        KeyCode::Up => app.previous_model(),
        KeyCode::Down => app.next_model(),
        KeyCode::Enter => app.toggle_model_selection(),
        KeyCode::Backspace
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Char(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_connect_mode(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_connect(),
        KeyCode::Up => app.previous_connect_provider(),
        KeyCode::Down => app.next_connect_provider(),
        KeyCode::Enter => app.select_connect_provider(),
        KeyCode::Backspace
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Char(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_api_key_input_mode(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_api_key_input(),
        KeyCode::Enter => app.save_api_key()?,
        KeyCode::Char(character) => app.add_api_key_char(character),
        KeyCode::Backspace => app.remove_api_key_char(),
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_chat_mode(app: &mut App, key_code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
    match (key_code, modifiers) {
        (KeyCode::Char('c'), key_modifiers) if key_modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true
        }
        (KeyCode::Char('r'), key_modifiers) if key_modifiers.contains(KeyModifiers::CONTROL) => {
            if let Err(error) = app.speak_last_response() {
                app.add_system_message(&format!("TTS Error: {}", error));
            } else if let Some(tts) = &app.tts_service {
                if tts.is_playing() {
                    app.show_status_toast("SPEAKING");
                } else {
                    app.show_status_toast("STOPPED");
                }
            }
        }
        (KeyCode::Char('t'), key_modifiers) if key_modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_auto_tts();
            let status = if app.auto_tts_enabled {
                "enabled"
            } else {
                "disabled"
            };
            app.add_system_message(&format!("Auto-TTS {}", status));
            if app.auto_tts_enabled {
                app.show_status_toast("TTS ACTIVE");
            } else {
                app.show_status_toast("TTS INACTIVE");
            }
        }
        (KeyCode::Char('p'), key_modifiers) if key_modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_personality();
        }
        (KeyCode::Char('v'), key_modifiers) if key_modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_chat_clipboard_image()?;
        }
        (KeyCode::Tab, _) => {
            // Rotate between chat and translate agents
            if let Err(error) = app.rotate_agent() {
                app.add_system_message(&format!("Failed to switch agent: {}", error));
            }
        }
        (KeyCode::Up, key_modifiers)
            if app.chat_input.is_empty() || key_modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.scroll_chat_up_lines(3);
        }
        (KeyCode::Down, key_modifiers)
            if app.chat_input.is_empty() || key_modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.scroll_chat_down_lines(3);
        }
        (KeyCode::PageUp, _) => app.scroll_chat_up_page(),
        (KeyCode::PageDown, _) => app.scroll_chat_down_page(),
        (KeyCode::End, _) => app.jump_to_bottom(),
        (KeyCode::Home, _) => app.jump_to_top(),
        (KeyCode::Char('/'), key_modifiers)
            if key_modifiers == KeyModifiers::NONE && app.chat_input.is_empty() =>
        {
            app.open_command_menu()
        }
        (KeyCode::Esc, _) => app.exit_chat_to_history()?,
        (KeyCode::Enter, _) => {
            app.send_chat_message()?;
            app.reset_chat_scroll();
        }
        (KeyCode::Char(character), _) => app.add_chat_input_char(character),
        (KeyCode::Backspace, _) => app.remove_chat_input_char(),
        (KeyCode::Left, _)
        | (KeyCode::Right, _)
        | (KeyCode::Up, _)
        | (KeyCode::Down, _)
        | (KeyCode::BackTab, _)
        | (KeyCode::Delete, _)
        | (KeyCode::Insert, _)
        | (KeyCode::F(_), _)
        | (KeyCode::Null, _)
        | (KeyCode::CapsLock, _)
        | (KeyCode::ScrollLock, _)
        | (KeyCode::NumLock, _)
        | (KeyCode::PrintScreen, _)
        | (KeyCode::Pause, _)
        | (KeyCode::Menu, _)
        | (KeyCode::KeypadBegin, _)
        | (KeyCode::Media(_), _)
        | (KeyCode::Modifier(_), _) => {}
    }
    Ok(())
}

fn handle_mouse_event(app: &mut App, mouse: event::MouseEvent) -> Result<()> {
    if app.mode != AppMode::Chat {
        return Ok(());
    }

    match mouse.kind {
        event::MouseEventKind::Down(event::MouseButton::Left) => {
            if is_in_chat_history(mouse.column, mouse.row)? {
                let message = app.last_assistant_message().map(str::to_string);
                if let Some(message) = message {
                    if app.clipboard_service.copy_text(&message).is_ok() {
                        app.show_status_toast("COPIED");
                    } else {
                        app.show_status_toast("COPY FAILED");
                    }
                }
            }
        }
        event::MouseEventKind::ScrollUp => {
            app.scroll_chat_up_lines(3);
        }
        event::MouseEventKind::ScrollDown => {
            app.scroll_chat_down_lines(3);
        }
        event::MouseEventKind::ScrollLeft | event::MouseEventKind::ScrollRight => {
            // Ignore horizontal scrolling
        }
        event::MouseEventKind::Down(_)
        | event::MouseEventKind::Up(_)
        | event::MouseEventKind::Drag(_)
        | event::MouseEventKind::Moved => {}
    }
    Ok(())
}

fn handle_paste(app: &mut App, paste: &str) -> Result<()> {
    let text = paste.replace('\n', "").replace('\r', "");
    if text.is_empty() {
        return Ok(());
    }

    match app.mode {
        AppMode::CommandMenu => {
            if app.handle_command_menu_paste(&text)? {
                return Ok(());
            }
            for character in text.chars() {
                app.add_input_char(character);
            }
        }
        AppMode::Chat => {
            app.handle_chat_paste(&text)?;
        }
        AppMode::ApiKeyInput => {
            for character in text.chars() {
                app.add_api_key_char(character);
            }
        }
        AppMode::History => {
            if app.history_filter_active {
                for character in text.chars() {
                    app.add_history_filter_char(character);
                }
            }
        }
        AppMode::PersonalityCreate => {
            for character in text.chars() {
                app.add_personality_char(character);
            }
        }
        AppMode::ModelSelection | AppMode::Connect | AppMode::Help | AppMode::PersonalitySelection => {}
    }

    Ok(())
}

fn is_in_chat_history(column: u16, row: u16) -> Result<bool> {
    let (width, height) = crossterm::terminal::size()?;
    let area = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Chat history
            Constraint::Length(3), // Input
            Constraint::Length(3), // Footer
        ])
        .split(area);

    let history_area = chunks
        .get(1)
        .copied()
        .ok_or_else(|| color_eyre::eyre::eyre!("Chat history area not found"))?;

    Ok(column >= history_area.x
        && column < history_area.x + history_area.width
        && row >= history_area.y
        && row < history_area.y + history_area.height)
}

fn handle_history_mode(app: &mut App, key_code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
    if app.history_delete_all_active {
        match key_code {
            KeyCode::Esc => app.cancel_history_delete_all(),
            KeyCode::Enter => app.confirm_history_delete_all()?,
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                app.toggle_history_delete_all_choice();
            }
            _ => {}
        }
        return Ok(());
    }
    let control_pressed = modifiers.contains(KeyModifiers::CONTROL);
    if app.history_filter_active {
        if control_pressed && key_code == KeyCode::Char('f') {
            app.toggle_history_filter();
            return Ok(());
        }
        match key_code {
            KeyCode::Esc => app.toggle_history_filter(),
            KeyCode::Char(character) => {
                if !control_pressed {
                    app.add_history_filter_char(character);
                }
            }
            KeyCode::Backspace => app.remove_history_filter_char(),
            KeyCode::Enter
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Delete
            | KeyCode::Insert
            | KeyCode::F(_)
            | KeyCode::Null
            | KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin
            | KeyCode::Media(_)
            | KeyCode::Modifier(_) => {}
        }
    } else {
        if control_pressed && key_code == KeyCode::Char('f') {
            app.toggle_history_filter();
            return Ok(());
        }
        if key_code == KeyCode::Delete && modifiers.contains(KeyModifiers::SHIFT) {
            app.open_history_delete_all();
            return Ok(());
        }
        match key_code {
            KeyCode::Esc => app.close_history(),
            KeyCode::Enter => app.load_history_conversation()?,
            KeyCode::Delete => app.delete_history_conversation()?,
            KeyCode::Char('/') => app.open_command_menu(),
            KeyCode::Char(character) => {
                if !control_pressed {
                    app.toggle_history_filter();
                    app.add_history_filter_char(character);
                }
            }
            KeyCode::Up => app.previous_history_item(),
            KeyCode::Down => app.next_history_item(),
            KeyCode::Backspace
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Insert
            | KeyCode::F(_)
            | KeyCode::Null
            | KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin
            | KeyCode::Media(_)
            | KeyCode::Modifier(_) => {}
        }
    }
    Ok(())
}

fn handle_help_mode(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_help(),
        KeyCode::Char('q') => app.close_help(),
        KeyCode::Enter
        | KeyCode::Backspace
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Char(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_personality_selection_mode(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.close_personality_menu(),
        KeyCode::Up => app.previous_personality(),
        KeyCode::Down => app.next_personality(),
        KeyCode::Enter => app.select_personality()?,
        KeyCode::Char('n') | KeyCode::Char('N') => app.open_personality_create(),
        KeyCode::Char('e') | KeyCode::Char('E') => app.edit_selected_personality()?,
        KeyCode::Delete => app.delete_selected_personality()?,
        KeyCode::Backspace
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Char(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}

fn handle_personality_create_mode(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Esc => app.open_personality_menu()?,
        KeyCode::Enter => app.create_personality()?,
        KeyCode::Char(character) => app.add_personality_char(character),
        KeyCode::Backspace => app.remove_personality_char(),
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => {}
    }
    Ok(())
}
