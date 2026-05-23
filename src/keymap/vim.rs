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
            editor.vim_on_key_event(key);

            if modifiers.contains(KeyModifiers::CONTROL) {
                match editor.state.mode {
                    Mode::Normal | Mode::Insert | Mode::Replace => match key {
                        KeyCode::Char('d') => {
                            editor.scroll_by(editor.terminal.rows() as usize / 2);
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('u') => {
                            editor.scroll_by(editor.terminal.rows() as usize / 2);
                            editor.needs_render = true;
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
                        KeyCode::Char('r') => {
                            if editor.state.mode == Mode::Normal {
                                editor.redo();
                                return;
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Capture mode BEFORE dispatching to prevent mode-transition keys
            // (like 'i', 'a', 'v', ':') from being re-dispatched to the new mode.
            let mode = editor.state.mode;
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
