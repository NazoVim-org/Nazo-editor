use crate::editor::Editor;

impl Editor {
    pub(crate) fn handle_mark(&mut self, action: char, name: char) {
        if action == 'm' {
            self.state.marks.set(name, self.state.cursor);
        } else if let Some(pos) = self.state.marks.get(name) {
            self.state.cursor = pos;
        }
    }

    pub(crate) fn play_macro(&mut self, name: char) {
        let keys = self.state.macros.get(name).cloned();
        if let Some(keys) = keys {
            for key_str in keys {
                self.execute_macro_key(&key_str);
            }
        }
    }

    /// Execute a single macro key stroke.
    ///
    /// NOTE: Currently only supports basic movement keys (h/j/k/l).
    /// Keys like i/a/x are parsed but their actions are no-ops
    /// because the macro system records keys as raw strings, not as
    /// `KeyCode` events through the full keymap pipeline. A future
    /// improvement should replay macros through the keymap handler
    /// (same path as normal key presses) instead of this standalone
    /// dispatch.
    fn execute_macro_key(&mut self, key_str: &str) {
        use crossterm::event::KeyCode;
        // All recognized keys; unknown keys are silently ignored.
        let key = match key_str {
            "h" | "j" | "k" | "l" | "i" | "a" | "x" => {
                KeyCode::Char(key_str.chars().next().unwrap())
            }
            _ => return,
        };
        match key {
            KeyCode::Char('j') => {
                self.state.cursor.line = (self.state.cursor.line + 1).min(self.buffer.line_count());
            }
            KeyCode::Char('k') => {
                self.state.cursor.line = self.state.cursor.line.saturating_sub(1).max(1);
            }
            KeyCode::Char('h') => {
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
            }
            KeyCode::Char('l') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = (self.state.cursor.col + 1).min(line_len.saturating_sub(1));
            }
            // i, a, x: recognized but currently no-ops (see note above).
            _ => {}
        }
        self.needs_render = true;
    }
}
