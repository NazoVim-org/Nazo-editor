use crate::editor::Editor;
use crate::types::{Mode, VisualType};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_visual(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                self.visual_yank();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('d') => {
                self.visual_delete();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('c') => {
                self.visual_delete();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('h') => {
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = (self.state.cursor.col + 1).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let line_count = self.buffer.line_count();
                self.state.cursor.line = (self.state.cursor.line + 1).min(line_count);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                self.state.cursor.line = self.state.cursor.line.saturating_sub(1).max(1);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let line = self.buffer.get_line(self.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let line = self.buffer.get_line(self.state.cursor.line);
                let line_len = line.len();
                self.state.cursor.col = if line_len > 0 && line.ends_with('\n') {
                    line_len.saturating_sub(2)
                } else {
                    line_len.saturating_sub(1)
                };
                self.needs_render = true;
            }
            KeyCode::Char('I') if self.state.visual_type == Some(VisualType::Block) => {
                let s_col = self
                    .state
                    .visual_start
                    .map(|s| s.col)
                    .unwrap_or(0)
                    .min(self.state.cursor.col);
                self.state.cursor.col = s_col;
                self.state.visual_start = None;
                self.state.visual_type = None;
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('A') if self.state.visual_type == Some(VisualType::Block) => {
                let e_col = self
                    .state
                    .visual_start
                    .map(|s| s.col)
                    .unwrap_or(0)
                    .max(self.state.cursor.col);
                self.state.cursor.col = e_col;
                self.state.visual_start = None;
                self.state.visual_type = None;
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            _ => {}
        }
    }

    fn visual_yank(&mut self) {
        if let (Some(start), Some(vtype)) = (&self.state.visual_start, self.state.visual_type) {
            let content = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(start, &self.state.cursor);
                    self.buffer.get_char_range(s_line, s_col, e_line, e_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.state.cursor.line);
                    self.buffer.get_line_range(s_line, e_line)
                }
                VisualType::Block => self.buffer.get_block_range(
                    start.line,
                    start.col,
                    self.state.cursor.line,
                    self.state.cursor.col,
                ),
            };
            self.register.set('"', &content);
        }
    }

    fn visual_delete(&mut self) {
        if let (Some(start), Some(vtype)) = (&self.state.visual_start, self.state.visual_type) {
            let (cursor_line, cursor_col) = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(start, &self.state.cursor);
                    let content = self.buffer.delete_range(s_line, s_col, e_line, e_col);
                    self.register.set('"', &content);
                    (s_line, s_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.state.cursor.line);
                    let count = e_line.saturating_sub(s_line) + 1;
                    let mut content = String::new();
                    for i in 0..count {
                        let deleted = self.buffer.delete_line(s_line);
                        content.push_str(&deleted);
                        if i < count - 1 {
                            content.push('\n');
                        }
                    }
                    self.register.set('"', &content);
                    (s_line.min(self.buffer.line_count()), 0)
                }
                VisualType::Block => {
                    let s_line = start.line.min(self.state.cursor.line);
                    let e_line = start.line.max(self.state.cursor.line);
                    let s_col = start.col.min(self.state.cursor.col);
                    let e_col = start.col.max(self.state.cursor.col);
                    let content = self.buffer.delete_block_range(s_line, s_col, e_line, e_col);
                    self.register.set('"', &content);
                    (s_line, s_col)
                }
            };
            self.state.cursor.line = cursor_line;
            self.state.cursor.col = cursor_col;
            self.on_buffer_modified();
        }
    }

    fn normalize_selection(
        &self,
        start: &crate::types::Position,
        end: &crate::types::Position,
    ) -> (usize, usize, usize, usize) {
        if start.line < end.line || (start.line == end.line && start.col <= end.col) {
            (start.line, start.col, end.line, end.col + 1)
        } else {
            (end.line, end.col, start.line, start.col + 1)
        }
    }

    fn normalize_line_selection(&self, start_line: usize, end_line: usize) -> (usize, usize) {
        if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        }
    }
}
