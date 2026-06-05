use crate::editor::Editor;
use crate::types::Mode;
use crossterm::event::{KeyCode, KeyModifiers};

impl Editor {
    pub(crate) fn handle_mark(&mut self, action: char, name: char) {
        if action == 'm' {
            self.engine.state.marks.set(name, self.engine.state.cursor);
        } else if let Some(pos) = self.engine.state.marks.get(name) {
            self.engine.state.cursor = pos;
        }
    }

    /// Play back a recorded macro.
    ///
    /// Each recorded key string is decoded back to a `(KeyCode, KeyModifiers)`
    /// pair and dispatched directly to the active mode handler.
    ///
    /// We avoid going through `handle_key` → `keymap_handler` to prevent a
    /// `RefCell` re-borrow panic (the outer `handle_key` already holds a
    /// borrow on `keymap_handler`).
    pub(crate) async fn play_macro(&mut self, name: char) {
        let keys = self.engine.state.macros.get(name).cloned();
        if let Some(keys) = keys {
            self.needs_render = true;
            for key_str in &keys {
                let Some((key, modifiers)) = Self::decode_key(key_str) else {
                    continue;
                };

                // Handle Ctrl modifier combinations that Vim normally
                // processes at the keymap level.
                if modifiers.contains(KeyModifiers::CONTROL)
                    && Self::handle_ctrl_at_keymap_level(self, key)
                {
                    continue;
                }

                // Dispatch to the current mode handler via a boxed future
                // to break potential async recursion (handle_normal may call
                // play_macro, but macro_play is already consumed by then).
                let fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>> =
                    match self.engine.state.mode {
                        Mode::Normal => Box::pin(self.handle_normal(key)),
                        Mode::Insert => Box::pin(self.handle_insert(key)),
                        Mode::Command => Box::pin(self.handle_command(key)),
                        Mode::Visual => Box::pin(self.handle_visual(key)),
                        Mode::Replace => Box::pin(self.handle_replace(key)),
                    };
                fut.await;
            }
        }
    }

    /// Handle Ctrl+ key combinations that are normally intercepted at the
    /// keymap level (before mode dispatch). Returns `true` if the key was
    /// consumed and no further mode dispatch is needed.
    ///
    /// This is a static helper (not `&mut self`) because it takes `&mut`
    /// indirectly through `Box<dyn FnOnce>` to avoid async recursion.
    fn handle_ctrl_at_keymap_level(editor: &mut Editor, key: KeyCode) -> bool {
        match editor.engine.state.mode {
            Mode::Normal | Mode::Insert | Mode::Replace => match key {
                KeyCode::Char('d') => {
                    editor.scroll_by(editor.terminal.rows() as usize / 2, true);
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('u') => {
                    if editor.engine.state.mode == Mode::Insert
                        || editor.engine.state.mode == Mode::Replace
                    {
                        editor.insert_delete_to_bol();
                    } else {
                        editor.scroll_by(editor.terminal.rows() as usize / 2, false);
                        editor.needs_render = true;
                    }
                    true
                }
                KeyCode::Char('w') => {
                    if editor.engine.state.mode == Mode::Insert
                        || editor.engine.state.mode == Mode::Replace
                    {
                        editor.insert_delete_word_backward();
                    }
                    true
                }
                KeyCode::Char('r') => {
                    if editor.engine.state.mode == Mode::Insert
                        || editor.engine.state.mode == Mode::Replace
                    {
                        editor.engine.insert_state.waiting_register = true;
                        editor.needs_render = true;
                    } else {
                        editor.redo();
                        editor.needs_render = true;
                    }
                    true
                }
                KeyCode::Char('y') => {
                    editor.scroll_up_one();
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('e') => {
                    editor.scroll_down_one();
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('g') => {
                    editor.show_file_info();
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('b') => {
                    editor.page_up();
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('f') => {
                    editor.page_down();
                    editor.needs_render = true;
                    true
                }
                KeyCode::Char('v') => {
                    let prev_mode = editor.engine.state.mode;
                    editor.engine.state.mode = Mode::Visual;
                    editor.engine.state.visual_start = Some(editor.engine.state.cursor);
                    editor.engine.state.visual_type = Some(crate::types::VisualType::Block);
                    editor
                        .engine
                        .plugin_manager
                        .emit(crate::types::PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Visual,
                        });
                    editor.needs_render = true;
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Decode a macro key string back into a `(KeyCode, KeyModifiers)` pair.
    ///
    /// Format: `{C-|M-}{key}` where the modifier prefix is optional.
    /// Reverse of `Engine::encode_key`.
    fn decode_key(s: &str) -> Option<(KeyCode, KeyModifiers)> {
        let (modifiers, rest) = if let Some(r) = s.strip_prefix("C-") {
            (KeyModifiers::CONTROL, r)
        } else if let Some(r) = s.strip_prefix("M-") {
            (KeyModifiers::ALT, r)
        } else {
            (KeyModifiers::NONE, s)
        };

        let key = match rest {
            "Enter" => KeyCode::Enter,
            "Esc" => KeyCode::Esc,
            "Backspace" => KeyCode::Backspace,
            "Tab" => KeyCode::Tab,
            "BackTab" => KeyCode::BackTab,
            "Delete" => KeyCode::Delete,
            "Insert" => KeyCode::Insert,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Null" => KeyCode::Null,
            s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
            s if s.starts_with('F') => {
                let n: u8 = s[1..].parse().ok()?;
                KeyCode::F(n)
            }
            _ => return None,
        };

        Some((key, modifiers))
    }
}
