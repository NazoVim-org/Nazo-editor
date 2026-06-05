use crate::editor::Editor;
use crate::types::DotAction;
use crate::types::{Mode, Operator, PendingOperator, PluginEvent, VisualType};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_normal(&mut self, key: KeyCode) {
        // ── Numeric count prefix ─────────────────────────────────
        // Accumulate digits 1-9 into pending_count.
        // '0' is only accumulated if there's already a pending count (e.g., "20j").
        // Standalone '0' falls through to the match arm for column-zero.
        if let KeyCode::Char(c) = key {
            if c.is_ascii_digit() && (c != '0' || self.engine.operator_state.count.is_some()) {
                let digit = c.to_digit(10).unwrap() as usize;
                self.engine.operator_state.count =
                    Some(self.engine.operator_state.count.unwrap_or(0) * 10 + digit);
                self.needs_render = true;
                return;
            }
        }

        // Resolve 3-key operator sequences (e.g., diw, da(, ciw)
        // and 2-key operator + motion sequences (e.g., dd, fw).
        // Single `replace` to avoid consuming operator state on text-object mismatch.
        let state = std::mem::replace(
            &mut self.engine.operator_state.pending,
            PendingOperator::Idle,
        );
        match state {
            PendingOperator::AwaitingTextObject {
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
            PendingOperator::AwaitingOperator { op, register: _ } => {
                let count = self.take_count();
                self.handle_operator(op, key, count).await;
                return;
            }
            PendingOperator::Idle => {}
        }

        // ── Macro playback (@x) ──────────────────────────────────
        // Check BEFORE the outer match because the macro name register
        // (a-z) overlaps with Vim normal-mode commands like `a` (append).
        if let Some(_pending) = self.engine.operator_state.macro_play {
            if let KeyCode::Char(c) = key {
                if c.is_ascii_lowercase() {
                    // Clear macro_play BEFORE dispatching so re-entrant
                    // handle_normal calls don't try to play another macro.
                    self.engine.operator_state.macro_play = None;
                    self.play_macro(c).await;
                    self.needs_render = true;
                    return;
                }
            }
            self.engine.operator_state.macro_play = None;
        }

        let line_count = self.engine.buffer.line_count();

        match key {
            // ── Motions with count support ───────────────────────
            KeyCode::Char('h') => {
                let count = self.take_count();
                self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(count);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let count = self.take_count();
                let line_len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col =
                    (self.engine.state.cursor.col + count).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    (self.engine.state.cursor.line + count).min(line_count);
                let len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    self.engine.state.cursor.line.saturating_sub(count).max(1);
                let len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('i') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('a') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                if self.engine.state.cursor.col
                    < self
                        .engine
                        .buffer
                        .get_line(self.engine.state.cursor.line)
                        .len()
                {
                    self.engine.state.cursor.col += 1;
                }
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('A') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                let line_len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col = line_len;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char(':') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('/') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.state.command_buffer.push('/');
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let _count = self.take_count();
                let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.engine.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let count = self.take_count();
                let target = (self.engine.state.cursor.line + count - 1)
                    .min(self.engine.buffer.line_count());
                let line = self.engine.buffer.get_line(target);
                let line_len = line.len();
                let end_col = if line_len > 0 && line.ends_with('\n') {
                    line_len.saturating_sub(2)
                } else {
                    line_len.saturating_sub(1)
                };
                self.engine.state.cursor.line = target;
                self.engine.state.cursor.col = end_col;
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
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('S') => {
                let _count = self.take_count();
                let content = self.engine.buffer.get_line(self.engine.state.cursor.line);
                self.engine.register.set('"', &content);
                self.engine
                    .buffer
                    .delete_line(self.engine.state.cursor.line);
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
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
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.state.command_buffer.push('?');
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('u') => {
                self.undo();
            }
            KeyCode::Char('r') => {
                if self.engine.state.command_buffer.is_empty() {
                    // Sentinel: single-char replace (r{char}) — wait for one char,
                    // replace, push undo, and exit back to Normal.
                    self.engine.operator_state.replace_char = Some('\0');
                    let prev_mode = self.engine.state.mode;
                    self.engine.state.mode = Mode::Replace;
                    self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Replace,
                    });
                    self.needs_render = true;
                }
            }
            // Multi-char Replace mode (R) — replace until Esc.
            KeyCode::Char('R') => {
                self.engine.operator_state.replace_char = None;
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Replace;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Replace,
                });
                self.needs_render = true;
            }
            KeyCode::Char('f') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::FindCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('F') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::FindCharBackward,
                        register,
                    };
                }
            }
            KeyCode::Char('t') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::TillCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('T') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
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
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Visual;
                self.engine.state.visual_start = Some(self.engine.state.cursor);
                self.engine.state.visual_type = Some(VisualType::Character);
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('V') => {
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Visual;
                self.engine.state.visual_start = Some(self.engine.state.cursor);
                self.engine.state.visual_type = Some(VisualType::Line);
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                let register = self.engine.operator_state.register.take();
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: Operator::Yank,
                    register,
                };
            }
            KeyCode::Char('d') => {
                let register = self.engine.operator_state.register.take();
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
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
                self.engine.operator_state.register = Some('"');
            }
            KeyCode::Char('m') => {
                self.engine.operator_state.mark = Some('m');
            }
            KeyCode::Char('`') => {
                self.engine.operator_state.mark = Some('`');
            }
            KeyCode::Char('\'') => {
                self.engine.operator_state.mark = Some('\'');
            }
            KeyCode::Char('@') => {
                self.engine.operator_state.macro_play = Some('@');
            }
            KeyCode::Char('q') => {
                if self.engine.state.command_buffer.is_empty() {
                    if self.engine.state.macros.is_recording() {
                        self.engine.state.macros.stop_recording();
                    } else {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::RecordMacro,
                            register,
                        };
                    }
                }
            }
            _ => {
                match key {
                    KeyCode::Char('g') => {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::Goto,
                            register,
                        };
                    }
                    KeyCode::Char('G') => {
                        let target = match self.engine.operator_state.count.take() {
                            Some(c) => c.min(self.engine.buffer.line_count()).max(1),
                            None => self.engine.buffer.line_count().max(1),
                        };
                        self.engine.state.cursor.line = target;
                        let len = self.engine.buffer.get_line(target).len();
                        self.engine.state.cursor.col = len.saturating_sub(1);
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
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
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
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::IndentRight,
                            register,
                        };
                    }
                    KeyCode::Char('<') => {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::IndentLeft,
                            register,
                        };
                    }
                    KeyCode::Char('z') => {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::Scroll,
                            register,
                        };
                    }
                    KeyCode::Char('Z') => {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::SaveQuit,
                            register,
                        };
                    }
                    KeyCode::Char('s') => {
                        let _count = self.take_count();
                        self.delete_char('"');
                        let prev_mode = self.engine.state.mode;
                        self.engine.state.mode = Mode::Insert;
                        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.needs_render = true;
                    }
                    _ => {}
                }

                if let Some(pending) = self.engine.operator_state.mark {
                    if let KeyCode::Char(c) = key {
                        if (pending == 'm' && c.is_ascii_lowercase())
                            || (pending == '`' || pending == '\'')
                        {
                            self.handle_mark(pending, c);
                            self.engine.operator_state.mark = None;
                            self.needs_render = true;
                            return;
                        }
                    }
                    self.engine.operator_state.mark = None;
                }
                if let Some(_pending_reg) = self.engine.operator_state.register {
                    if let KeyCode::Char(c) = key {
                        if c.is_ascii_lowercase() {
                            self.engine.operator_state.register = Some(c);
                            return;
                        }
                    }
                    self.engine.operator_state.register = None;
                }
            }
        }
    }

    pub(crate) async fn handle_operator(&mut self, op: Operator, key: KeyCode, count: usize) {
        let register = self.engine.operator_state.register.unwrap_or('"');
        self.engine.operator_state.register = None;

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
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Yank,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
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
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Delete,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
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
                        let content = self.engine.buffer.get_line(self.engine.state.cursor.line);
                        self.engine.register.set(register, &content);
                        self.engine
                            .buffer
                            .delete_line(self.engine.state.cursor.line);
                    }
                    let prev_mode = self.engine.state.mode;
                    self.engine.state.mode = Mode::Insert;
                    self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                    self.engine.dot_last_action = Some(DotAction::Change {
                        text: String::new(),
                        line: self.engine.state.cursor.line,
                        col: 0,
                    });
                    self.on_buffer_modified();
                }
                KeyCode::Char('w') => {
                    let (word, start, _) = self.engine.buffer.get_word_range(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col,
                    );
                    if !word.is_empty() {
                        let char_start = self
                            .engine
                            .buffer
                            .line_to_char(self.engine.state.cursor.line - 1)
                            + start;
                        let char_end = char_start + word.len();
                        let content = self.engine.buffer.get_char_range(
                            self.engine.state.cursor.line,
                            start,
                            self.engine.state.cursor.line,
                            start + word.len(),
                        );
                        self.engine.register.set(register, &content);
                        self.engine.buffer.remove_range(char_start, char_end);
                        let prev_mode = self.engine.state.mode;
                        self.engine.state.mode = Mode::Insert;
                        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.engine.dot_last_action = Some(DotAction::Change {
                            text: content,
                            line: self.engine.state.cursor.line,
                            col: start,
                        });
                        self.on_buffer_modified();
                    }
                }
                KeyCode::Char('i') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Change,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
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
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = false;
                    if self.find_char(ch, false, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::FindCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = false;
                    if self.find_char(ch, false, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharForward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = true;
                    if self.find_char(ch, true, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = true;
                    if self.find_char(ch, true, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::RecordMacro => {
                if let KeyCode::Char(c) = key {
                    if c.is_ascii_lowercase() {
                        self.engine.state.macros.start_recording(c);
                    }
                }
            }
            Operator::Goto => {
                if let KeyCode::Char('g') = key {
                    self.engine.state.cursor.line = 1;
                    self.engine.state.cursor.col = 0;
                    self.needs_render = true;
                } else if let KeyCode::Char('v') = key {
                    // gv: restore last visual selection
                    if let (Some(start), Some(vtype)) = (
                        self.engine.state.last_visual_start,
                        self.engine.state.last_visual_type,
                    ) {
                        self.engine.state.visual_start = Some(start);
                        self.engine.state.visual_type = Some(vtype);
                        self.engine.state.cursor = start;
                        let prev_mode = self.engine.state.mode;
                        self.engine.state.mode = Mode::Visual;
                        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Visual,
                        });
                        self.needs_render = true;
                    }
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
                    if !self.engine.buffer.is_dirty() {
                        self.running = false;
                    }
                }
                self.needs_render = true;
            }
        }
    }

    fn yank_line(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let content = self.engine.buffer.get_line_range(line, line);
        self.engine.register.set(register, &content);
        self.needs_render = true;
    }

    fn yank_word(&mut self, register: char) {
        let (word, _, _) = self
            .engine
            .buffer
            .get_word_range(self.engine.state.cursor.line, self.engine.state.cursor.col);
        self.engine.register.set(register, &word);
        self.needs_render = true;
    }

    fn yank_word_end(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let word: String = chars[col..end].iter().collect();
        self.engine.register.set(register, &word);
        self.needs_render = true;
    }

    fn delete_line(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let mod_count = self.engine.buffer.modification_count();
        let content = self.engine.buffer.delete_line(line);
        self.engine.register.set(register, &content);

        self.engine.undo_manager.push(Edit {
            edit_type: EditType::DeleteLine {
                line,
                text: content.clone(),
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });

        let line_count = self.engine.buffer.line_count();
        if line > line_count {
            self.engine.state.cursor.line = line_count.max(1);
        }
        let len = self
            .engine
            .buffer
            .get_line(self.engine.state.cursor.line)
            .len();
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(len.saturating_sub(1));
        self.on_buffer_modified();
    }

    fn delete_word(&mut self, register: char) {
        let (word, start, _) = self
            .engine
            .buffer
            .get_word_range(self.engine.state.cursor.line, self.engine.state.cursor.col);
        if !word.is_empty() {
            let char_start = self
                .engine
                .buffer
                .line_to_char(self.engine.state.cursor.line - 1)
                + start;
            let char_end = char_start + word.len();
            self.engine.buffer.remove_range(char_start, char_end);
            self.engine.register.set(register, &word);
        }
        self.needs_render = true;
    }

    async fn paste(&mut self, before: bool) {
        let content = self.engine.register.get_default();
        if content.is_empty() {
            return;
        }

        if content.contains('\n') {
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    if before {
                        self.engine
                            .buffer
                            .insert(self.engine.state.cursor.line, 0, line);
                        self.engine
                            .buffer
                            .insert(self.engine.state.cursor.line, line.len(), "\n");
                    } else {
                        self.engine.buffer.insert(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col,
                            line,
                        );
                        self.engine.buffer.insert(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col + line.len(),
                            "\n",
                        );
                    }
                } else {
                    let insert_line = self.engine.state.cursor.line + i;
                    self.engine.buffer.insert(insert_line, 0, line);
                    self.engine.buffer.insert(insert_line, line.len(), "\n");
                }
            }
            if before {
                self.engine.state.cursor.line += lines.len() - 1;
                self.engine.state.cursor.col = lines.last().map(|l| l.len()).unwrap_or(0);
            } else {
                self.engine.state.cursor.line += lines.len() - 1;
                let last_line = lines.last().unwrap();
                self.engine.state.cursor.col = last_line.len();
            }
        } else {
            if before {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    &content,
                );
            } else {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col + 1,
                    &content,
                );
                self.engine.state.cursor.col += 1;
            }
            if !before {
                self.engine.state.cursor.col += content.len();
            }
        }

        self.on_buffer_modified();
    }

    fn delete_char(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let mod_count = self.engine.buffer.modification_count();

        if col >= line_str.len() {
            if line < self.engine.buffer.line_count() {
                // Merge with next line (EOL x behavior)
                self.engine.buffer.merge_with_prev_line(line + 1);
                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Merge {
                        line,
                        deleted_newline_col: col,
                    },
                    cursor_before: self.engine.state.cursor,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
                self.engine.dot_last_action = Some(DotAction::Delete {
                    text: String::new(),
                    line: 0,
                    col: 0,
                });
            }
        } else {
            let deleted_text: String = line_str[col..].chars().take(1).collect();
            self.engine.buffer.delete(line, col);
            self.engine.register.set(register, &deleted_text);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col,
                    text: deleted_text.clone(),
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.engine.dot_last_action = Some(DotAction::Delete {
                text: String::new(),
                line: 0,
                col: 0,
            });
        }

        self.on_buffer_modified();
    }

    async fn repeat_last_action(&mut self) {
        if let Some(action) = &self.engine.dot_last_action {
            match action.clone() {
                DotAction::Insert { text } => {
                    for ch in text.chars() {
                        self.engine.buffer.insert_char(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col,
                            ch,
                        );
                        self.engine.state.cursor.col += 1;
                    }
                    self.on_buffer_modified();
                }
                DotAction::Delete { text: _, line: _, col: _ } => {
                    // Re-delete one char at current cursor position.
                    self.delete_char('"');
                    self.on_buffer_modified();
                }
                DotAction::Change { text, line, col } => {
                    self.engine.buffer.insert(line, col, &text);
                    self.engine.state.cursor.line = line;
                    self.engine.state.cursor.col = col;
                    self.on_buffer_modified();
                }
                DotAction::Replace { ch } => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let line_str = self.engine.buffer.get_line(line);
                    if col < line_str.len() {
                        let start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + col;
                        let end = start + 1;
                        self.engine.buffer.remove_range(start, end);
                        self.engine.buffer.insert_char(line, col, ch);
                        self.on_buffer_modified();
                    }
                }
            }
        }
    }

    pub(crate) fn open_line(&mut self, above: bool) {
        let line = self.engine.state.cursor.line;

        if above {
            self.engine.buffer.insert(line, 0, "\n");
            self.engine.state.cursor.line = line;
        } else {
            self.engine.buffer.insert(line + 1, 0, "\n");
            self.engine.state.cursor.line = line + 1;
        }
        self.engine.state.cursor.col = 0;

        let prev_mode = self.engine.state.mode;
        self.engine.state.mode = Mode::Insert;
        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
            from: prev_mode,
            to: Mode::Insert,
        });

        self.engine.dot_last_action = Some(DotAction::Insert {
            text: String::new(),
        });
        self.on_buffer_modified();
    }

    pub(crate) fn join_lines(&mut self) {
        let line = self.engine.state.cursor.line;
        if line >= self.engine.buffer.line_count() {
            return;
        }

        let mod_count = self.engine.buffer.modification_count();
        let current_line = self.engine.buffer.get_line(line);
        let current_stripped = current_line.trim_end_matches('\n');
        let next_line = self.engine.buffer.get_line(line + 1);
        let next_stripped = next_line.trim_end_matches('\n');

        let join_pos = self.engine.buffer.merge_with_prev_line(line + 1);

        let needs_space = !current_stripped.ends_with(' ')
            && !current_stripped.ends_with('\t')
            && !next_stripped.starts_with(' ')
            && !next_stripped.is_empty();
        if needs_space {
            self.engine.buffer.insert(line, join_pos, " ");
            self.engine.state.cursor.col = join_pos + 1;
        } else {
            self.engine.state.cursor.col = join_pos;
        }

        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Merge {
                line,
                deleted_newline_col: join_pos,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });

        self.engine.dot_last_action = Some(DotAction::Change {
            text: String::new(),
            line: 0,
            col: 0,
        });
        self.on_buffer_modified();
    }
}
