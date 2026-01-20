impl crate::app::App {
    pub fn scroll_chat_up_lines(&mut self, lines: usize) {
        // Disable auto-scroll when user manually scrolls up
        self.chat_auto_scroll = false;
        self.show_status_toast("SCROLLED");
        // Increase offset to scroll toward older messages
        self.chat_scroll_offset = self.chat_scroll_offset.saturating_add(lines);
    }

    pub fn scroll_chat_down_lines(&mut self, lines: usize) {
        self.show_status_toast("SCROLLED");
        if self.chat_scroll_offset > 0 {
            self.chat_scroll_offset = self.chat_scroll_offset.saturating_sub(lines);
        }
        // Re-enable auto-scroll when reaching the bottom
        if self.chat_scroll_offset == 0 {
            self.chat_auto_scroll = true;
        }
    }

    pub fn scroll_chat_up_page(&mut self) {
        // Page up - scroll ~20 lines
        self.scroll_chat_up_lines(20);
    }

    pub fn scroll_chat_down_page(&mut self) {
        // Page down - scroll ~20 lines
        self.scroll_chat_down_lines(20);
    }

    pub fn jump_to_top(&mut self) {
        // Jump to top
        self.chat_auto_scroll = false;
        self.show_status_toast("SCROLLED");
        self.chat_scroll_offset = 10000; // Large number, will be clamped in render
    }

    pub fn reset_chat_scroll(&mut self) {
        // Reset to bottom and enable auto-scroll
        self.chat_scroll_offset = 0;
        self.chat_auto_scroll = true;
    }

    pub fn jump_to_bottom(&mut self) {
        // Explicitly jump to bottom
        self.show_status_toast("SCROLLED");
        self.chat_scroll_offset = 0;
        self.chat_auto_scroll = true;
    }

    pub fn toggle_auto_tts(&mut self) {
        self.auto_tts_enabled = !self.auto_tts_enabled;
    }
}
