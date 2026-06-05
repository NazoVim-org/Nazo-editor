use crate::editor::Editor;
use crate::types::Mode;
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_insert(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                // Push accumulated insert text as a single undo group
                if !self.engine.insert_state.accum.is_empty() {
                    let text = std::mem::take(&mut self.engine.insert_state.accum);
                    let cursor_before = self
                        .engine
                        .insert_state
                        .start_pos
                        .unwrap_or(crate::types::Position { line: 1, col: 0 });
                    self.engine.insert_state.start_pos = None;
                    let mod_count = self.engine.buffer.modification_count();
                    self.engine.undo_manager.push(crate::undo::Edit {
                        edit_type: crate::undo::EditType::Insert {
                            line: cursor_before.line,
                            col: cursor_before.col,
                            text,
                        },
                        cursor_before,
                        cursor_after: self.engine.state.cursor,
                        modification_count: mod_count,
                    });
                }
                self.engine.insert_state.start_pos = None;
                self.transition_mode(Mode::Normal);
                self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                if self.engine.state.cursor.col > 0 {
                    self.engine.buffer.delete(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col - 1,
                    );
                    self.engine.state.cursor.col -= 1;
                    self.engine.insert_state.accum.pop();
                    self.on_buffer_modified();
                } else if self.engine.state.cursor.line > 1 {
                    let new_col = self
                        .engine
                        .buffer
                        .merge_with_prev_line(self.engine.state.cursor.line);
                    self.engine.state.cursor.line -= 1;
                    self.engine.state.cursor.col = new_col;
                    // Remove the last newline from accumulator
                    self.engine.insert_state.accum.pop();
                    self.on_buffer_modified();
                }
            }
            KeyCode::Enter => {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    "\n",
                );
                self.engine.state.cursor.line += 1;
                self.engine.state.cursor.col = 0;
                self.engine.insert_state.accum.push('\n');
                self.on_buffer_modified();
            }
            KeyCode::Char(c) => {
                self.engine.buffer.insert_char(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    c,
                );
                self.engine.state.cursor.col += 1;
                self.engine.insert_state.accum.push(c);
                self.on_buffer_modified();
            }
            _ => {}
        }
    }

    pub(crate) async fn handle_replace(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.transition_mode(Mode::Normal);
                self.engine.operator_state.replace_char = None;
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                let line = self.engine.state.cursor.line;
                let col = self.engine.state.cursor.col;
                if col < self.engine.buffer.get_line(line).len() {
                    self.engine.buffer.delete(line, col);
                }
                self.engine.buffer.insert_char(line, col, c);
                self.engine.state.cursor.col += 1;
                self.on_buffer_modified();
            }
            KeyCode::Backspace => {
                if self.engine.state.cursor.col > 0 {
                    self.engine.state.cursor.col -= 1;
                }
                self.needs_render = true;
            }
            KeyCode::Enter => {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    "\n",
                );
                self.engine.state.cursor.line += 1;
                self.engine.state.cursor.col = 0;
                self.on_buffer_modified();
            }
            _ => {}
        }
    }
}
