use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crossterm::event::{KeyCode, KeyModifiers};

/// Keymap handler for Review mode.
///
/// Review mode replaces the regular Vim/Emacs keymap while active.
/// All keybindings are custom for the review workflow:
/// file list navigation, diff browsing, staging, commenting, etc.
pub struct ReviewKeymap;

impl ReviewKeymap {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapHandler for ReviewKeymap {
    fn handle_key<'e>(
        &self,
        editor: &'e mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'e>> {
        Box::pin(async move {
            // Check for exit keys first — always available regardless of sub-mode.
            match key {
                KeyCode::Char('q') if modifiers.is_empty() => {
                    editor.exit_review_mode();
                    return;
                }
                KeyCode::Esc => {
                    match editor.engine.review_state.as_ref().map(|s| s.sub_mode) {
                        Some(crate::review::ReviewSubMode::Help) => {
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = crate::review::ReviewSubMode::FileList;
                            }
                            editor.needs_render = true;
                        }
                        Some(crate::review::ReviewSubMode::DiffView) => {
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = crate::review::ReviewSubMode::FileList;
                            }
                            editor.needs_render = true;
                        }
                        Some(crate::review::ReviewSubMode::CommentInput) => {
                            // Abort comment
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = crate::review::ReviewSubMode::DiffView;
                            }
                            editor.needs_render = true;
                        }
                        // Already in FileList or Preview: exit review mode
                        _ => {
                            editor.exit_review_mode();
                        }
                    }
                    return;
                }
                _ => {}
            }

            // Dispatch based on current review sub-mode.
            let sub_mode = editor
                .engine
                .review_state
                .as_ref()
                .map(|s| s.sub_mode)
                .unwrap_or(crate::review::ReviewSubMode::FileList);

            match sub_mode {
                crate::review::ReviewSubMode::FileList => {
                    handle_file_list(editor, key, modifiers);
                }
                crate::review::ReviewSubMode::DiffView => {
                    handle_diff_view(editor, key, modifiers);
                }
                crate::review::ReviewSubMode::Preview => {
                    handle_preview(editor, key, modifiers);
                }
                crate::review::ReviewSubMode::CommentInput => {
                    handle_comment_input(editor, key, modifiers);
                }
                crate::review::ReviewSubMode::Help => {
                    // Any key dismisses help
                    if let Some(state) = &mut editor.engine.review_state {
                        state.sub_mode = crate::review::ReviewSubMode::FileList;
                    }
                    editor.needs_render = true;
                }
            }
        })
    }
}

// ── File List sub-mode ───────────────────────────────────────────────

fn handle_file_list(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    let state = match editor.engine.review_state.as_mut() {
        Some(s) => s,
        None => return,
    };

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            if state.selected_file + 1 < state.files.len() {
                state.selected_file += 1;
                state.selected_hunk = 0;
            }
            editor.needs_render = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if state.selected_file > 0 {
                state.selected_file -= 1;
                state.selected_hunk = 0;
            }
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            // Enter diff view for the selected file
            state.sub_mode = crate::review::ReviewSubMode::DiffView;
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            // Toggle diff style
            state.diff_style = match state.diff_style {
                crate::review::DiffStyle::Unified => crate::review::DiffStyle::SideBySide,
                crate::review::DiffStyle::SideBySide => crate::review::DiffStyle::Unified,
            };
            editor.needs_render = true;
        }
        KeyCode::Char('?') => {
            state.sub_mode = crate::review::ReviewSubMode::Help;
            editor.needs_render = true;
        }
        _ => {}
    }
}

// ── Diff View sub-mode ───────────────────────────────────────────────

fn handle_diff_view(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    let state = match editor.engine.review_state.as_mut() {
        Some(s) => s,
        None => return,
    };

    let file = match state.files.get_mut(state.selected_file) {
        Some(f) => f,
        None => return,
    };

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_add(1);
            editor.needs_render = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_sub(1);
            editor.needs_render = true;
        }
        KeyCode::Char(']') | KeyCode::Char(')') => {
            // Next hunk
            if state.selected_hunk + 1 < file.hunks.len() {
                state.selected_hunk += 1;
            }
            editor.needs_render = true;
        }
        KeyCode::Char('[') | KeyCode::Char('(') => {
            // Previous hunk
            if state.selected_hunk > 0 {
                state.selected_hunk -= 1;
            }
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            // Toggle hunk staged state
            if state.selected_hunk < file.hunks.len() {
                let hunk = &mut file.hunks[state.selected_hunk];
                let file_path = &file.new_path;

                if hunk.staged {
                    // Unstage
                    if state.project_root.is_some() {
                        match crate::review::diff::unstage_hunk(hunk, file_path) {
                            Err(e) => editor.engine.state.set_message(e),
                            Ok(()) => hunk.staged = false,
                        }
                    } else {
                        hunk.staged = false;
                    }
                } else {
                    // Stage
                    if state.project_root.is_some() {
                        match crate::review::diff::stage_hunk(hunk, file_path) {
                            Err(e) => editor.engine.state.set_message(e),
                            Ok(()) => hunk.staged = true,
                        }
                    } else {
                        hunk.staged = true;
                    }
                }
            }
            editor.needs_render = true;
        }
        KeyCode::Char('o') | KeyCode::Char('p') => {
            // Open preview for the current hunk
            state.sub_mode = crate::review::ReviewSubMode::Preview;
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            // Toggle diff style
            state.diff_style = match state.diff_style {
                crate::review::DiffStyle::Unified => crate::review::DiffStyle::SideBySide,
                crate::review::DiffStyle::SideBySide => crate::review::DiffStyle::Unified,
            };
            editor.needs_render = true;
        }
        KeyCode::Char('?') => {
            state.sub_mode = crate::review::ReviewSubMode::Help;
            editor.needs_render = true;
        }
        _ => {}
    }
}

// ── Preview sub-mode ─────────────────────────────────────────────────

fn handle_preview(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    match key {
        KeyCode::Esc => {
            if let Some(state) = &mut editor.engine.review_state {
                state.sub_mode = crate::review::ReviewSubMode::DiffView;
            }
            editor.needs_render = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(state) = &mut editor.engine.review_state {
                state.scroll.scroll_top = state.scroll.scroll_top.saturating_add(1);
            }
            editor.needs_render = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(state) = &mut editor.engine.review_state {
                state.scroll.scroll_top = state.scroll.scroll_top.saturating_sub(1);
            }
            editor.needs_render = true;
        }
        _ => {}
    }
}

// ── Comment Input sub-mode ───────────────────────────────────────────

fn handle_comment_input(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    match key {
        KeyCode::Esc => {
            if let Some(state) = &mut editor.engine.review_state {
                state.sub_mode = crate::review::ReviewSubMode::DiffView;
            }
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            // TODO: save comment
            if let Some(state) = &mut editor.engine.review_state {
                state.sub_mode = crate::review::ReviewSubMode::DiffView;
            }
            editor.needs_render = true;
        }
        KeyCode::Char(_c) => {
            // TODO: append to comment buffer
            editor.needs_render = true;
        }
        _ => {}
    }
}
