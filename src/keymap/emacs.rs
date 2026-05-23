use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::Mode;
use crate::types::PluginEvent;
use crossterm::event::{KeyCode, KeyModifiers};

pub struct EmacsKeymap {
    prefix_state: EmacsPrefixState,
    pending_save: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum EmacsPrefixState {
    None,
    WaitingCx,
}

impl EmacsKeymap {
    pub fn new() -> Self {
        Self {
            prefix_state: EmacsPrefixState::None,
            pending_save: false,
        }
    }
}

impl KeymapHandler for EmacsKeymap {
    fn handle_key<'a>(
        &'a mut self,
        editor: &'a mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
        Box::pin(async move {
            if self.pending_save {
                self.pending_save = false;
                editor.pending_save = true;
            }

            let has_ctrl = modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = modifiers.contains(KeyModifiers::ALT);

            if let KeyCode::Char(c) = key {
                editor.plugin_manager.emit(PluginEvent::Key {
                    mode: editor.state.mode,
                    key: c,
                });
            }

            if editor.state.has_confirmation() {
                editor.handle_confirmation(key).await;
                return;
            }

            match self.prefix_state {
                EmacsPrefixState::WaitingCx => {
                    self.prefix_state = EmacsPrefixState::None;
                    match (has_ctrl, key) {
                        (true, KeyCode::Char('s')) => {
                            self.pending_save = true;
                            return;
                        }
                        (true, KeyCode::Char('c')) => {
                            editor.handle_quit();
                            return;
                        }
                        (true, KeyCode::Char('h')) => {
                            editor.cursor_line_start();
                            return;
                        }
                        (true, KeyCode::Char('d')) => {
                            editor.cursor_line_end();
                            return;
                        }
                        // C-x u / C-x C-/: redo
                        (false, KeyCode::Char('u')) => {
                            editor.redo();
                            editor.needs_render = true;
                            return;
                        }
                        (true, KeyCode::Char('/')) => {
                            editor.redo();
                            editor.needs_render = true;
                            return;
                        }
                        _ => {}
                    }
                }
                EmacsPrefixState::None => {}
            }

            match (has_ctrl, key) {
                (true, KeyCode::Char('x')) => {
                    self.prefix_state = EmacsPrefixState::WaitingCx;
                }
                (true, KeyCode::Char('f')) => {
                    editor.cursor_right(1);
                }
                (true, KeyCode::Char('b')) => {
                    editor.cursor_left(1);
                }
                (true, KeyCode::Char('n')) => {
                    editor.cursor_down(1);
                }
                (true, KeyCode::Char('p')) => {
                    editor.cursor_up(1);
                }
                (true, KeyCode::Char('a')) => {
                    editor.cursor_line_start();
                }
                (true, KeyCode::Char('e')) => {
                    editor.cursor_line_end();
                }
                (true, KeyCode::Char('d')) => {
                    editor.delete_char_forward();
                }
                (true, KeyCode::Char('k')) => {
                    editor.kill_line();
                }
                (true, KeyCode::Char('h')) => {
                    editor.delete_char_backward();
                }
                (true, KeyCode::Char(' ')) | (true, KeyCode::Char('@')) => {
                    editor.set_mark();
                }
                (true, KeyCode::Char('w')) => {
                    if editor.has_active_region() {
                        editor.kill_region();
                    } else {
                        editor.kill_word();
                    }
                }
                (true, KeyCode::Char('y')) => {
                    editor.yank_from_kill_ring();
                }
                (true, KeyCode::Char('o')) => {
                    // Spec decision: keep C-o as a legacy direct save trigger.
                    // Preferred Emacs-style save is C-x C-s, but C-o remains supported
                    // for compatibility with existing nestvim behavior.
                    editor.pending_save = true;
                }
                (true, KeyCode::Char('s')) => {
                    let prev_mode = editor.state.mode;
                    editor.state.mode = Mode::Command;
                    editor.state.command_buffer.clear();
                    editor.state.command_buffer.push('/');
                    editor.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Command,
                    });
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('t')) => {
                    editor.transpose_chars();
                }
                (true, KeyCode::Char('v')) => {
                    editor.scroll_up_one();
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('l')) => {
                    editor.clear_screen();
                    editor.scroll_cursor_to_center();
                }
                (true, KeyCode::Char('g')) => {
                    editor.deactivate_region();
                    editor.abort();
                }
                (true, KeyCode::Char('/')) => {
                    editor.undo();
                }
                (true, KeyCode::Char('_')) => {
                    editor.undo();
                }
                (true, KeyCode::Char('?')) => {
                    editor.undo();
                }
                (false, KeyCode::Char(c)) if !c.is_control() => {
                    editor.insert_char(c);
                }
                (false, KeyCode::Backspace) => {
                    editor.delete_char_backward();
                }
                (false, KeyCode::Delete) => {
                    editor.delete_char_forward();
                }
                (false, KeyCode::Enter) => {
                    editor.insert_newline();
                }
                (false, KeyCode::Tab) => {
                    editor.insert_tab();
                }
                (false, KeyCode::Up) => {
                    editor.cursor_up(1);
                }
                (false, KeyCode::Down) => {
                    editor.cursor_down(1);
                }
                (false, KeyCode::Left) => {
                    editor.cursor_left(1);
                }
                (false, KeyCode::Right) => {
                    editor.cursor_right(1);
                }
                (false, KeyCode::Home) => {
                    editor.cursor_line_start();
                }
                (false, KeyCode::End) => {
                    editor.cursor_line_end();
                }
                (false, KeyCode::PageUp) => {
                    editor.scroll_up();
                }
                (false, KeyCode::PageDown) => {
                    editor.scroll_down();
                }
                _ => {
                    // Alt/Meta keybindings
                    if has_alt {
                        match key {
                            KeyCode::Char('f') => {
                                editor.move_word_forward();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('b') => {
                                editor.move_word_backward();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('v') => {
                                editor.scroll_down();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('w') => {
                                if editor.has_active_region() {
                                    editor.copy_region();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
    }
}

impl Default for EmacsKeymap {
    fn default() -> Self {
        Self::new()
    }
}
