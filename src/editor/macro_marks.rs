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

    fn execute_macro_key(&mut self, key_str: &str) {
        use crossterm::event::KeyCode;
        let key = match key_str {
            "h" => KeyCode::Char('h'),
            "j" => KeyCode::Char('j'),
            "k" => KeyCode::Char('k'),
            "l" => KeyCode::Char('l'),
            "i" => KeyCode::Char('i'),
            "a" => KeyCode::Char('a'),
            "x" => KeyCode::Char('x'),
            "dd" => KeyCode::Char('d'),
            "yy" => KeyCode::Char('y'),
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
            _ => {}
        }
        self.needs_render = true;
    }
}
