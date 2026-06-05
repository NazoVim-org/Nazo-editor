use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::Mode;
use crossterm::event::{KeyCode, KeyModifiers};

pub struct VimKeymap;

impl VimKeymap {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VimKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapHandler for VimKeymap {
    fn handle_key<'a>(
        &'a mut self,
        editor: &'a mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
        Box::pin(async move {
            editor.emit_key_event(key, modifiers);

            if modifiers.contains(KeyModifiers::CONTROL) {
                match editor.engine.state.mode {
                    Mode::Normal | Mode::Insert | Mode::Replace => match key {
                        KeyCode::Char('d') => {
                            editor.scroll_by(editor.terminal.rows() as usize / 2, true);
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('u') => {
                            // Normal: scroll half-pages up; Insert: delete to BOL
                            if editor.engine.state.mode == Mode::Insert
                                || editor.engine.state.mode == Mode::Replace
                            {
                                editor.insert_delete_to_bol();
                            } else {
                                editor.scroll_by(editor.terminal.rows() as usize / 2, false);
                                editor.needs_render = true;
                            }
                            return;
                        }
                        KeyCode::Char('w') => {
                            // Ctrl-w: delete word backward in Insert mode
                            if editor.engine.state.mode == Mode::Insert
                                || editor.engine.state.mode == Mode::Replace
                            {
                                editor.insert_delete_word_backward();
                            }
                            return;
                        }
                        KeyCode::Char('r') => {
                            // Normal: redo; Insert: insert register (Ctrl-r)
                            if editor.engine.state.mode == Mode::Insert
                                || editor.engine.state.mode == Mode::Replace
                            {
                                editor.engine.insert_state.waiting_register = true;
                                editor.needs_render = true;
                            } else {
                                editor.redo();
                                editor.needs_render = true;
                            }
                            return;
                        }
                        KeyCode::Char('y') => {
                            editor.scroll_up_one();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('e') => {
                            editor.scroll_down_one();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('g') => {
                            editor.show_file_info();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('b') => {
                            editor.page_up();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('f') => {
                            editor.page_down();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('v') => {
                            // Ctrl-v: enter visual block mode
                            let prev_mode = editor.engine.state.mode;
                            editor.engine.state.mode = Mode::Visual;
                            editor.engine.state.visual_start = Some(editor.engine.state.cursor);
                            editor.engine.state.visual_type = Some(crate::types::VisualType::Block);
                            editor.engine.plugin_manager.emit(
                                crate::types::PluginEvent::ModeChange {
                                    from: prev_mode,
                                    to: Mode::Visual,
                                },
                            );
                            editor.needs_render = true;
                            return;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Capture mode BEFORE dispatching to prevent mode-transition keys
            // (like 'i', 'a', 'v', ':') from being re-dispatched to the new mode.
            let mode = editor.engine.state.mode;
            match mode {
                Mode::Normal => editor.handle_normal(key).await,
                Mode::Insert => editor.handle_insert(key).await,
                Mode::Command => editor.handle_command(key).await,
                Mode::Visual => editor.handle_visual(key).await,
                Mode::Replace => editor.handle_replace(key).await,
            }
        })
    }
}
