use crate::app::{App, AppMode};

impl App {
    pub fn open_help(&mut self) {
        self.mode = AppMode::Help;
    }

    pub fn close_help(&mut self) {
        self.mode = AppMode::Chat;
    }
}
