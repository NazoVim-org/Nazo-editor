use crate::editor::Editor;
use crate::types::Position;
use crate::undo::{Edit, EditType};

impl Editor {
    pub fn set_mark(&mut self) {
        self.state.mark = Some(self.state.cursor);
        self.state.region_active = true;
        self.needs_render = true;
    }

    pub fn has_active_region(&self) -> bool {
        self.state.region_active
            && self.state.mark.is_some()
            && self.state.mark.unwrap() != self.state.cursor
    }

    pub fn kill_region(&mut self) {
        let mark = match self.state.mark {
            Some(m) => m,
            None => return,
        };
        let cursor = self.state.cursor;

        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };

        if e_line == s_line && e_col <= s_col {
            self.deactivate_region();
            return;
        }

        let mod_count = self.buffer.modification_count();
        let content = self.buffer.delete_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }

        self.kill_ring.push(&content);
        self.register.set('"', &content);
        self.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: s_line,
                col: s_col,
                text: content,
            },
            cursor_before: cursor,
            cursor_after: Position {
                line: s_line,
                col: s_col,
            },
            modification_count: mod_count,
        });
        self.state.cursor = Position {
            line: s_line,
            col: s_col,
        };
        self.deactivate_region();
        self.on_buffer_modified();
    }

    pub fn deactivate_region(&mut self) {
        if self.state.region_active {
            self.state.region_active = false;
            self.needs_render = true;
        }
    }

    pub fn copy_region(&mut self) {
        let mark = match self.state.mark {
            Some(m) => m,
            None => return,
        };
        let cursor = self.state.cursor;
        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };
        let content = self.buffer.get_char_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }
        self.kill_ring.push(&content);
        self.register.set('"', &content);
        self.deactivate_region();
        self.needs_render = true;
    }

    pub fn yank_from_kill_ring(&mut self) {
        let text = self.kill_ring.yank().map(|s| s.to_string());
        if let Some(ref text) = text {
            self.yank_start = Some(self.state.cursor);
            self.yank_length = text.chars().count();
            self.kill_ring.reset_index();
            self.buffer
                .insert(self.state.cursor.line, self.state.cursor.col, text);
            self.state.cursor.col += self.yank_length;
            self.on_buffer_modified();
        }
    }

    pub fn yank_pop(&mut self) {
        let start = match self.yank_start {
            Some(p) => p,
            None => return,
        };
        let len = self.yank_length;
        if len == 0 {
            return;
        }
        let start_char = self.buffer.line_to_char(start.line.saturating_sub(1)) + start.col;
        self.buffer.remove_range(start_char, start_char + len);
        let next = self.kill_ring.yank_pop().map(|s| s.to_string());
        if let Some(ref text) = next {
            self.buffer.insert(start.line, start.col, text);
            self.yank_length = text.chars().count();
            self.state.cursor = start;
            self.state.cursor.col += self.yank_length;
        } else {
            self.yank_start = None;
            self.yank_length = 0;
        }
        self.on_buffer_modified();
    }

    pub fn transpose_chars(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;

        if col > 0 {
            let line_str = self.buffer.get_line(line);
            let chars: Vec<char> = line_str.chars().collect();
            if col < chars.len() {
                let c1 = chars[col - 1];
                let c2 = chars[col];

                self.buffer.delete(line, col);
                self.buffer.delete(line, col - 1);
                self.buffer.insert_char(line, col - 1, c2);
                self.buffer.insert_char(line, col, c1);
                self.state.cursor.col =
                    (col + 1).min(self.buffer.get_line(line).len().saturating_sub(1));
                self.on_buffer_modified();
            }
        }
    }

    pub fn insert_tab(&mut self) {
        self.buffer
            .insert_char(self.state.cursor.line, self.state.cursor.col, '\t');
        self.state.cursor.col += 1;
        self.on_buffer_modified();
    }

    pub fn clear_screen(&mut self) {
        let _ = self.terminal.clear_screen();
        self.needs_render = true;
    }

    pub fn abort(&mut self) {
        self.state.command_buffer.clear();
        if self.state.mode == crate::types::Mode::Command {
            self.state.mode = crate::types::Mode::Normal;
            self.needs_render = true;
        }
    }

    pub fn kill_word(&mut self) {
        let (_, _, end_col) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        if end_col > self.state.cursor.col {
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line,
                end_col,
            );
            self.on_buffer_modified();
        } else if self.state.cursor.line < self.buffer.line_count() {
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line + 1,
                0,
            );
            self.on_buffer_modified();
        }
    }

    pub(crate) fn kill_line(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        let line_len = line.len();
        let end_col = if line_len > 0 && line.ends_with('\n') {
            line_len - 1
        } else {
            line_len
        };
        if self.state.cursor.col < end_col {
            let deleted = line[self.state.cursor.col..end_col].to_string();
            self.kill_ring.push(&deleted);
            let mod_count = self.buffer.modification_count();
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line,
                end_col,
            );
            self.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line: self.state.cursor.line,
                    col: self.state.cursor.col,
                    text: deleted,
                },
                cursor_before: self.state.cursor,
                cursor_after: self.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        self.buffer
            .insert_char(self.state.cursor.line, self.state.cursor.col, c);
        self.state.cursor.col += 1;
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_backward(&mut self) {
        if self.state.cursor.col > 0 {
            self.buffer
                .delete(self.state.cursor.line, self.state.cursor.col - 1);
            self.state.cursor.col -= 1;
            self.on_buffer_modified();
        } else if self.state.cursor.line > 1 {
            let merged = self.buffer.get_line(self.state.cursor.line - 1);
            let prev_len = merged.len();
            self.buffer.merge_with_prev_line(self.state.cursor.line);
            self.state.cursor.line -= 1;
            self.state.cursor.col = prev_len.saturating_sub(1);
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_newline(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        let (_, after) = line.split_at(self.state.cursor.col);
        self.buffer
            .insert(self.state.cursor.line, self.state.cursor.col, "\n");
        if !after.is_empty() {
            self.buffer.insert(self.state.cursor.line + 1, 0, after);
        }
        self.state.cursor.line += 1;
        self.state.cursor.col = 0;
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_forward(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        if self.state.cursor.col < line.len() {
            let deleted_char = line[self.state.cursor.col..self.state.cursor.col + 1].to_string();
            let mod_count = self.buffer.modification_count();
            self.buffer
                .delete(self.state.cursor.line, self.state.cursor.col);
            self.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line: self.state.cursor.line,
                    col: self.state.cursor.col,
                    text: deleted_char,
                },
                cursor_before: self.state.cursor,
                cursor_after: self.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }
}
