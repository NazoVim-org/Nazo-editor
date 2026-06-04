use crate::editor::Editor;
use crate::types::Mode;
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_insert(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                // Push accumulated insert text as a single undo group
                if !self.insert_accum.is_empty() {
                    let text = std::mem::take(&mut self.insert_accum);
                    let cursor_before = self
                        .insert_start_pos
                        .unwrap_or(crate::types::Position { line: 1, col: 0 });
                    self.insert_start_pos = None;
                    let mod_count = self.buffer.modification_count();
                    self.undo_manager.push(crate::undo::Edit {
                        edit_type: crate::undo::EditType::Insert {
                            line: cursor_before.line,
                            col: cursor_before.col,
                            text,
                        },
                        cursor_before,
                        cursor_after: self.state.cursor,
                        modification_count: mod_count,
                    });
                }
                self.insert_start_pos = None;
                self.transition_mode(Mode::Normal);
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                if self.state.cursor.col > 0 {
                    self.buffer
                        .delete(self.state.cursor.line, self.state.cursor.col - 1);
                    self.state.cursor.col -= 1;
                    self.insert_accum.pop();
                    self.on_buffer_modified();
                } else if self.state.cursor.line > 1 {
                    let new_col = self.buffer.merge_with_prev_line(self.state.cursor.line);
                    self.state.cursor.line -= 1;
                    self.state.cursor.col = new_col;
                    // Remove the last newline from accumulator
                    self.insert_accum.pop();
                    self.on_buffer_modified();
                }
            }
            KeyCode::Enter => {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col, "\n");
                self.state.cursor.line += 1;
                self.state.cursor.col = 0;
                self.insert_accum.push('\n');
                self.on_buffer_modified();
            }
            KeyCode::Char(c) => {
                self.buffer
                    .insert_char(self.state.cursor.line, self.state.cursor.col, c);
                self.state.cursor.col += 1;
                self.insert_accum.push(c);
                self.on_buffer_modified();
            }
            _ => {}
        }
    }

    pub(crate) async fn handle_replace(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.transition_mode(Mode::Normal);
                self.replace_char = None;
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                let line = self.state.cursor.line;
                let col = self.state.cursor.col;
                if col < self.buffer.get_line(line).len() {
                    self.buffer.delete(line, col);
                }
                self.buffer.insert_char(line, col, c);
                self.state.cursor.col += 1;
                self.on_buffer_modified();
            }
            KeyCode::Backspace => {
                if self.state.cursor.col > 0 {
                    self.state.cursor.col -= 1;
                }
                self.needs_render = true;
            }
            KeyCode::Enter => {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col, "\n");
                self.state.cursor.line += 1;
                self.state.cursor.col = 0;
                self.on_buffer_modified();
            }
            _ => {}
        }
    }
}
