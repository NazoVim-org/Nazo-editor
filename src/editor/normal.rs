use crate::editor::{DotAction, Editor};
use crate::types::{CommandState, Mode, Operator, PluginEvent, VisualType};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_normal(&mut self, key: KeyCode) {
        // ── Numeric count prefix ─────────────────────────────────
        // Accumulate digits 1-9 into pending_count.
        // '0' is only accumulated if there's already a pending count (e.g., "20j").
        // Standalone '0' falls through to the match arm for column-zero.
        if let KeyCode::Char(c) = key {
            if c.is_ascii_digit() && (c != '0' || self.pending_count.is_some()) {
                let digit = c.to_digit(10).unwrap() as usize;
                self.pending_count = Some(self.pending_count.unwrap_or(0) * 10 + digit);
                self.needs_render = true;
                return;
            }
        }

        // Resolve 3-key operator sequences (e.g., diw, da(, ciw)
        // and 2-key operator + motion sequences (e.g., dd, fw).
        // Single `replace` to avoid consuming operator state on text-object mismatch.
        let state = std::mem::replace(&mut self.command_state, CommandState::Idle);
        match state {
            CommandState::AwaitingTextObject {
                op,
                register,
                inner,
            } => {
                match op {
                    Operator::Yank | Operator::Delete => {
                        self.handle_text_object(key, register, inner).await;
                    }
                    Operator::Change => {
                        self.handle_text_object_change(key, register, inner).await;
                    }
                    _ => {}
                }
                return;
            }
            CommandState::AwaitingOperator { op, register: _ } => {
                let count = self.take_count();
                self.handle_operator(op, key, count).await;
                return;
            }
            CommandState::Idle => {}
        }

        let line_count = self.buffer.line_count();

        match key {
            // ── Motions with count support ───────────────────────
            KeyCode::Char('h') => {
                let count = self.take_count();
                self.state.cursor.col = self.state.cursor.col.saturating_sub(count);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let count = self.take_count();
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col =
                    (self.state.cursor.col + count).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let count = self.take_count();
                self.state.cursor.line = (self.state.cursor.line + count).min(line_count);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                let count = self.take_count();
                self.state.cursor.line = self.state.cursor.line.saturating_sub(count).max(1);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('i') => {
                let _count = self.take_count();
                let prev_mode = self.state.mode;
                self.insert_accum.clear();
                self.insert_start_pos = Some(self.state.cursor);
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('a') => {
                let _count = self.take_count();
                let prev_mode = self.state.mode;
                self.insert_accum.clear();
                self.insert_start_pos = Some(self.state.cursor);
                if self.state.cursor.col < self.buffer.get_line(self.state.cursor.line).len() {
                    self.state.cursor.col += 1;
                }
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('A') => {
                let _count = self.take_count();
                let prev_mode = self.state.mode;
                self.insert_accum.clear();
                self.insert_start_pos = Some(self.state.cursor);
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = line_len;
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char(':') => {
                let _count = self.take_count();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Command;
                self.state.command_buffer.clear();
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('/') => {
                let _count = self.take_count();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Command;
                self.state.command_buffer.clear();
                self.state.command_buffer.push('/');
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let _count = self.take_count();
                let line = self.buffer.get_line(self.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let count = self.take_count();
                let target = (self.state.cursor.line + count - 1).min(self.buffer.line_count());
                let line = self.buffer.get_line(target);
                let line_len = line.len();
                let end_col = if line_len > 0 && line.ends_with('\n') {
                    line_len.saturating_sub(2)
                } else {
                    line_len.saturating_sub(1)
                };
                self.state.cursor.line = target;
                self.state.cursor.col = end_col;
                self.needs_render = true;
            }
            KeyCode::Char('D') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.kill_line();
                }
            }
            KeyCode::Char('C') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.kill_line();
                }
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('S') => {
                let _count = self.take_count();
                let content = self.buffer.get_line(self.state.cursor.line);
                self.register.set('"', &content);
                self.buffer.delete_line(self.state.cursor.line);
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.on_buffer_modified();
            }
            KeyCode::Char('*') => {
                let _count = self.take_count();
                self.search_word_under_cursor();
                self.needs_render = true;
            }
            KeyCode::Char('n') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.search_next();
                }
                self.needs_render = true;
            }
            KeyCode::Char('N') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.search_prev();
                }
                self.needs_render = true;
            }
            KeyCode::Char('?') => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Command;
                self.state.command_buffer.clear();
                self.state.command_buffer.push('?');
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('u') => {
                self.undo();
            }
            KeyCode::Char('r') => {
                if self.state.command_buffer.is_empty() {
                    let prev_mode = self.state.mode;
                    self.state.mode = Mode::Replace;
                    self.replace_char = None;
                    self.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Replace,
                    });
                    self.needs_render = true;
                }
            }
            KeyCode::Char('R') => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Replace;
                self.replace_char = None;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Replace,
                });
                self.needs_render = true;
            }
            KeyCode::Char('f') => {
                if self.state.command_buffer.is_empty() {
                    let register = self.pending_register.take();
                    self.command_state = CommandState::AwaitingOperator {
                        op: Operator::FindCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('F') => {
                if self.state.command_buffer.is_empty() {
                    let register = self.pending_register.take();
                    self.command_state = CommandState::AwaitingOperator {
                        op: Operator::FindCharBackward,
                        register,
                    };
                }
            }
            KeyCode::Char('t') => {
                if self.state.command_buffer.is_empty() {
                    let register = self.pending_register.take();
                    self.command_state = CommandState::AwaitingOperator {
                        op: Operator::TillCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('T') => {
                if self.state.command_buffer.is_empty() {
                    let register = self.pending_register.take();
                    self.command_state = CommandState::AwaitingOperator {
                        op: Operator::TillCharBackward,
                        register,
                    };
                }
            }
            KeyCode::Char(';') => {
                if self.repeat_find(true) {
                    self.needs_render = true;
                }
            }
            KeyCode::Char(',') => {
                if self.repeat_find(false) {
                    self.needs_render = true;
                }
            }
            KeyCode::Char('v') => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Visual;
                self.state.visual_start = Some(self.state.cursor);
                self.state.visual_type = Some(VisualType::Character);
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('V') => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Visual;
                self.state.visual_start = Some(self.state.cursor);
                self.state.visual_type = Some(VisualType::Line);
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                let register = self.pending_register.take();
                self.command_state = CommandState::AwaitingOperator {
                    op: Operator::Yank,
                    register,
                };
            }
            KeyCode::Char('d') => {
                let register = self.pending_register.take();
                self.command_state = CommandState::AwaitingOperator {
                    op: Operator::Delete,
                    register,
                };
            }
            KeyCode::Char('p') => {
                self.paste(false).await;
            }
            KeyCode::Char('P') => {
                self.paste(true).await;
            }
            KeyCode::Char('"') => {
                self.pending_register = Some('"');
            }
            KeyCode::Char('m') => {
                self.pending_mark = Some('m');
            }
            KeyCode::Char('`') => {
                self.pending_mark = Some('`');
            }
            KeyCode::Char('\'') => {
                self.pending_mark = Some('\'');
            }
            KeyCode::Char('@') => {
                self.pending_macro_play = Some('@');
            }
            KeyCode::Char('q') => {
                if self.state.command_buffer.is_empty() {
                    if self.state.macros.is_recording() {
                        self.state.macros.stop_recording();
                    } else {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::RecordMacro,
                            register,
                        };
                    }
                }
            }
            _ => {
                match key {
                    KeyCode::Char('g') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::Goto,
                            register,
                        };
                    }
                    KeyCode::Char('G') => {
                        let target = match self.pending_count.take() {
                            Some(c) => c.min(self.buffer.line_count()).max(1),
                            None => self.buffer.line_count().max(1),
                        };
                        self.state.cursor.line = target;
                        let len = self.buffer.get_line(target).len();
                        self.state.cursor.col = len.saturating_sub(1);
                        self.needs_render = true;
                    }
                    KeyCode::Char('%') => {
                        let _count = self.take_count();
                        self.jump_to_matching_bracket();
                        self.needs_render = true;
                    }
                    KeyCode::Char('x') => {
                        let count = self.take_count();
                        for _ in 0..count {
                            self.delete_char('"');
                        }
                        self.needs_render = true;
                    }
                    KeyCode::Char('.') => {
                        let _count = self.take_count();
                        self.repeat_last_action().await;
                        self.needs_render = true;
                    }
                    KeyCode::Char('w') => {
                        let count = self.take_count();
                        for _ in 0..count {
                            self.move_word_forward();
                        }
                        self.needs_render = true;
                    }
                    KeyCode::Char('b') => {
                        let count = self.take_count();
                        for _ in 0..count {
                            self.move_word_backward();
                        }
                        self.needs_render = true;
                    }
                    KeyCode::Char('e') => {
                        let count = self.take_count();
                        for _ in 0..count {
                            self.move_word_end();
                        }
                        self.needs_render = true;
                    }
                    KeyCode::Char('o') => {
                        let _count = self.take_count();
                        self.open_line(false);
                        self.needs_render = true;
                    }
                    KeyCode::Char('O') => {
                        let _count = self.take_count();
                        self.open_line(true);
                        self.needs_render = true;
                    }
                    KeyCode::Char('c') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::Change,
                            register,
                        };
                    }
                    KeyCode::Char('J') => {
                        let count = self.take_count();
                        for _ in 0..count {
                            self.join_lines();
                        }
                        self.needs_render = true;
                    }
                    KeyCode::Char('>') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::IndentRight,
                            register,
                        };
                    }
                    KeyCode::Char('<') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::IndentLeft,
                            register,
                        };
                    }
                    KeyCode::Char('z') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::Scroll,
                            register,
                        };
                    }
                    KeyCode::Char('Z') => {
                        let register = self.pending_register.take();
                        self.command_state = CommandState::AwaitingOperator {
                            op: Operator::SaveQuit,
                            register,
                        };
                    }
                    KeyCode::Char('s') => {
                        let _count = self.take_count();
                        self.delete_char('"');
                        let prev_mode = self.state.mode;
                        self.state.mode = Mode::Insert;
                        self.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.needs_render = true;
                    }
                    _ => {}
                }

                if let Some(_pending) = self.pending_macro_play {
                    if let KeyCode::Char(c) = key {
                        if c.is_ascii_lowercase() {
                            self.play_macro(c);
                            self.pending_macro_play = None;
                            self.needs_render = true;
                            return;
                        }
                    }
                    self.pending_macro_play = None;
                }
                if let Some(pending) = self.pending_mark {
                    if let KeyCode::Char(c) = key {
                        if (pending == 'm' && c.is_ascii_lowercase())
                            || (pending == '`' || pending == '\'')
                        {
                            self.handle_mark(pending, c);
                            self.pending_mark = None;
                            self.needs_render = true;
                            return;
                        }
                    }
                    self.pending_mark = None;
                }
                if let Some(_pending_reg) = self.pending_register {
                    if let KeyCode::Char(c) = key {
                        if c.is_ascii_lowercase() {
                            self.pending_register = Some(c);
                            return;
                        }
                    }
                    self.pending_register = None;
                }
            }
        }
    }

    pub(crate) async fn handle_operator(&mut self, op: Operator, key: KeyCode, count: usize) {
        let register = self.pending_register.unwrap_or('"');
        self.pending_register = None;

        match op {
            Operator::Yank => match key {
                KeyCode::Char('y') => {
                    for _ in 0..count {
                        self.yank_line(register);
                    }
                }
                KeyCode::Char('w') => {
                    self.yank_word(register);
                }
                KeyCode::Char('e') => {
                    self.yank_word_end(register);
                }
                KeyCode::Char('i') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Yank,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Yank,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::Delete => match key {
                KeyCode::Char('d') => {
                    for _ in 0..count.max(1) {
                        self.delete_line(register);
                    }
                }
                KeyCode::Char('w') => {
                    self.delete_word(register);
                }
                KeyCode::Char('i') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Delete,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Delete,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::Change => match key {
                KeyCode::Char('c') => {
                    for _ in 0..count {
                        let content = self.buffer.get_line(self.state.cursor.line);
                        self.register.set(register, &content);
                        self.buffer.delete_line(self.state.cursor.line);
                    }
                    let prev_mode = self.state.mode;
                    self.state.mode = Mode::Insert;
                    self.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                    self.dot_last_action = Some(DotAction::Change {
                        text: String::new(),
                        line: self.state.cursor.line,
                        col: 0,
                    });
                    self.on_buffer_modified();
                }
                KeyCode::Char('w') => {
                    let (word, start, _) = self
                        .buffer
                        .get_word_range(self.state.cursor.line, self.state.cursor.col);
                    if !word.is_empty() {
                        let char_start =
                            self.buffer.line_to_char(self.state.cursor.line - 1) + start;
                        let char_end = char_start + word.len();
                        let content = self.buffer.get_char_range(
                            self.state.cursor.line,
                            start,
                            self.state.cursor.line,
                            start + word.len(),
                        );
                        self.register.set(register, &content);
                        self.buffer.remove_range(char_start, char_end);
                        let prev_mode = self.state.mode;
                        self.state.mode = Mode::Insert;
                        self.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.dot_last_action = Some(DotAction::Change {
                            text: content,
                            line: self.state.cursor.line,
                            col: start,
                        });
                        self.on_buffer_modified();
                    }
                }
                KeyCode::Char('i') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Change,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.command_state = CommandState::AwaitingTextObject {
                        op: Operator::Change,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::IndentRight => {
                if let KeyCode::Char('>') = key {
                    for _ in 0..count {
                        self.indent_lines(register, true);
                    }
                }
            }
            Operator::IndentLeft => {
                if let KeyCode::Char('<') = key {
                    for _ in 0..count {
                        self.indent_lines(register, false);
                    }
                }
            }
            Operator::FindCharForward => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = false;
                    if self.find_char(ch, false, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::FindCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = false;
                    if self.find_char(ch, false, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharForward => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = true;
                    if self.find_char(ch, true, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = true;
                    if self.find_char(ch, true, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::RecordMacro => {
                if let KeyCode::Char(c) = key {
                    if c.is_ascii_lowercase() {
                        self.state.macros.start_recording(c);
                    }
                }
            }
            Operator::Goto => {
                if let KeyCode::Char('g') = key {
                    self.state.cursor.line = 1;
                    self.state.cursor.col = 0;
                    self.needs_render = true;
                }
            }
            Operator::Scroll => {
                match key {
                    KeyCode::Char('z') => {
                        self.scroll_cursor_to_center();
                    }
                    KeyCode::Char('t') => {
                        self.scroll_cursor_to_top();
                    }
                    KeyCode::Char('b') => {
                        self.scroll_cursor_to_bottom();
                    }
                    _ => {}
                }
                self.needs_render = true;
            }
            Operator::SaveQuit => {
                if let KeyCode::Char('Z') = key {
                    self.save_file_async().await;
                    if !self.buffer.dirty {
                        self.running = false;
                    }
                }
                self.needs_render = true;
            }
        }
    }

    fn yank_line(&mut self, register: char) {
        let line = self.state.cursor.line;
        let content = self.buffer.get_line_range(line, line);
        self.register.set(register, &content);
        self.needs_render = true;
    }

    fn yank_word(&mut self, register: char) {
        let (word, _, _) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        self.register.set(register, &word);
        self.needs_render = true;
    }

    fn yank_word_end(&mut self, register: char) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let word: String = chars[col..end].iter().collect();
        self.register.set(register, &word);
        self.needs_render = true;
    }

    fn delete_line(&mut self, register: char) {
        let line = self.state.cursor.line;
        let mod_count = self.buffer.modification_count();
        let content = self.buffer.delete_line(line);
        self.register.set(register, &content);

        self.undo_manager.push(Edit {
            edit_type: EditType::DeleteLine {
                line,
                text: content.clone(),
            },
            cursor_before: self.state.cursor,
            cursor_after: self.state.cursor,
            modification_count: mod_count,
        });

        let line_count = self.buffer.line_count();
        if line > line_count {
            self.state.cursor.line = line_count.max(1);
        }
        let len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
        self.on_buffer_modified();
    }

    fn delete_word(&mut self, register: char) {
        let (word, start, _) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        if !word.is_empty() {
            let char_start = self.buffer.line_to_char(self.state.cursor.line - 1) + start;
            let char_end = char_start + word.len();
            self.buffer.remove_range(char_start, char_end);
            self.register.set(register, &word);
        }
        self.needs_render = true;
    }

    async fn paste(&mut self, before: bool) {
        let content = self.register.get_default();
        if content.is_empty() {
            return;
        }

        if content.contains('\n') {
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    if before {
                        self.buffer.insert(self.state.cursor.line, 0, line);
                        self.buffer.insert(self.state.cursor.line, line.len(), "\n");
                    } else {
                        self.buffer
                            .insert(self.state.cursor.line, self.state.cursor.col, line);
                        self.buffer.insert(
                            self.state.cursor.line,
                            self.state.cursor.col + line.len(),
                            "\n",
                        );
                    }
                } else {
                    let insert_line = self.state.cursor.line + i;
                    self.buffer.insert(insert_line, 0, line);
                    self.buffer.insert(insert_line, line.len(), "\n");
                }
            }
            if before {
                self.state.cursor.line += lines.len() - 1;
                self.state.cursor.col = lines.last().map(|l| l.len()).unwrap_or(0);
            } else {
                self.state.cursor.line += lines.len() - 1;
                let last_line = lines.last().unwrap();
                self.state.cursor.col = last_line.len();
            }
        } else {
            if before {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col, &content);
            } else {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col + 1, &content);
                self.state.cursor.col += 1;
            }
            if !before {
                self.state.cursor.col += content.len();
            }
        }

        self.on_buffer_modified();
    }

    fn delete_char(&mut self, _register: char) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);

        if col >= line_str.len() {
            if line < self.buffer.line_count() {
                self.buffer.merge_with_prev_line(line + 1);
            }
        } else {
            self.buffer.delete(line, col);
        }

        self.dot_last_action = Some(DotAction::Delete {
            text: String::new(),
            line,
            col,
        });
        self.on_buffer_modified();
    }

    async fn repeat_last_action(&mut self) {
        if let Some(action) = &self.dot_last_action {
            match action.clone() {
                DotAction::Insert { text } => {
                    for ch in text.chars() {
                        self.buffer
                            .insert_char(self.state.cursor.line, self.state.cursor.col, ch);
                        self.state.cursor.col += 1;
                    }
                    self.on_buffer_modified();
                }
                DotAction::Delete { text, line, col } => {
                    if !text.is_empty() {
                        self.buffer.insert(line, col, &text);
                        self.on_buffer_modified();
                    }
                }
                DotAction::Change { text, line, col } => {
                    self.buffer.insert(line, col, &text);
                    self.state.cursor.line = line;
                    self.state.cursor.col = col;
                    self.on_buffer_modified();
                }
            }
        }
    }

    pub(crate) fn open_line(&mut self, above: bool) {
        let line = self.state.cursor.line;

        if above {
            self.buffer.insert(line, 0, "\n");
            self.state.cursor.line = line;
        } else {
            self.buffer.insert(line + 1, 0, "\n");
            self.state.cursor.line = line + 1;
        }
        self.state.cursor.col = 0;

        let prev_mode = self.state.mode;
        self.state.mode = Mode::Insert;
        self.plugin_manager.emit(PluginEvent::ModeChange {
            from: prev_mode,
            to: Mode::Insert,
        });

        self.dot_last_action = Some(DotAction::Insert {
            text: String::new(),
        });
        self.on_buffer_modified();
    }

    pub(crate) fn join_lines(&mut self) {
        let line = self.state.cursor.line;
        if line >= self.buffer.line_count() {
            return;
        }

        let current_line = self.buffer.get_line(line);
        let current_stripped = current_line.trim_end_matches('\n');
        let next_line = self.buffer.get_line(line + 1);
        let next_stripped = next_line.trim_end_matches('\n');

        let join_pos = self.buffer.merge_with_prev_line(line + 1);

        let needs_space = !current_stripped.ends_with(' ')
            && !current_stripped.ends_with('\t')
            && !next_stripped.starts_with(' ')
            && !next_stripped.is_empty();
        if needs_space {
            self.buffer.insert(line, join_pos, " ");
            self.state.cursor.col = join_pos + 1;
        } else {
            self.state.cursor.col = join_pos;
        }

        self.dot_last_action = Some(DotAction::Change {
            text: String::new(),
            line,
            col: 0,
        });
        self.on_buffer_modified();
    }
}
