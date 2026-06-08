use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::review::annotations;
use crate::review::Annotation;
use crate::review::{DiffLineKind, DiffStyle, ReviewSubMode};
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
                        Some(ReviewSubMode::Help) => {
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = ReviewSubMode::FileList;
                            }
                            editor.needs_render = true;
                        }
                        Some(ReviewSubMode::DiffView) => {
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = ReviewSubMode::FileList;
                            }
                            editor.needs_render = true;
                        }
                        Some(ReviewSubMode::CommentInput) => {
                            // Abort comment
                            if let Some(state) = &mut editor.engine.review_state {
                                state.sub_mode = ReviewSubMode::DiffView;
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
                .unwrap_or(ReviewSubMode::FileList);

            match sub_mode {
                ReviewSubMode::FileList => {
                    handle_file_list(editor, key, modifiers);
                }
                ReviewSubMode::DiffView => {
                    handle_diff_view(editor, key, modifiers);
                }
                ReviewSubMode::Preview => {
                    handle_preview(editor, key, modifiers);
                }
                ReviewSubMode::CommentInput => {
                    handle_comment_input(editor, key, modifiers);
                }
                ReviewSubMode::Help => {
                    // Any key dismisses help
                    if let Some(state) = &mut editor.engine.review_state {
                        state.sub_mode = ReviewSubMode::FileList;
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
            state.sub_mode = ReviewSubMode::DiffView;
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            // Toggle diff style
            state.diff_style = match state.diff_style {
                DiffStyle::Unified => DiffStyle::SideBySide,
                DiffStyle::SideBySide => DiffStyle::Unified,
            };
            editor.needs_render = true;
        }
        KeyCode::Char('?') => {
            state.sub_mode = ReviewSubMode::Help;
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
        KeyCode::Char('c') => {
            // Enter CommentInput sub-mode for the current line
            state.comment_buffer.clear();
            state.comment_cursor = 0;
            state.sub_mode = ReviewSubMode::CommentInput;
            editor.needs_render = true;
        }
        KeyCode::Char('o') | KeyCode::Char('p') => {
            // Open preview for the current hunk
            state.sub_mode = ReviewSubMode::Preview;
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            // Toggle diff style
            state.diff_style = match state.diff_style {
                DiffStyle::Unified => DiffStyle::SideBySide,
                DiffStyle::SideBySide => DiffStyle::Unified,
            };
            editor.needs_render = true;
        }
        KeyCode::Char('?') => {
            state.sub_mode = ReviewSubMode::Help;
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
                state.sub_mode = ReviewSubMode::DiffView;
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
    let state = match editor.engine.review_state.as_mut() {
        Some(s) => s,
        None => return,
    };

    match key {
        KeyCode::Esc => {
            // Cancel — discard buffer
            state.comment_buffer.clear();
            state.comment_cursor = 0;
            state.sub_mode = ReviewSubMode::DiffView;
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            // Save the comment as an annotation
            let text = state.comment_buffer.trim().to_string();
            if !text.is_empty() {
                let file = match state.files.get(state.selected_file) {
                    Some(f) => f,
                    None => return,
                };
                let hunk = match file.hunks.get(state.selected_hunk) {
                    Some(h) => h,
                    None => return,
                };
                // Find the first added/deleted line as the target line
                let target_line = hunk
                    .lines
                    .iter()
                    .find(|l| matches!(l.kind, DiffLineKind::Add | DiffLineKind::Delete))
                    .and_then(|l| l.new_lineno.or(l.old_lineno))
                    .unwrap_or(hunk.new_start);

                let annotation = Annotation {
                    id: annotations::generate_annotation_id(),
                    file_path: file.new_path.clone(),
                    line: target_line,
                    text,
                    resolved: false,
                    created_at: format!("{:?}", std::time::SystemTime::now()),
                };

                // Persist if we have a project root
                if let Some(ref root) = state.project_root {
                    match annotations::add_annotation(root, annotation.clone()) {
                        Ok(anns) => {
                            state.annotations = anns;
                            editor.engine.state.set_message("Annotation saved");
                        }
                        Err(e) => {
                            // Still add to in-memory state even if file write fails
                            state.annotations.push(annotation);
                            editor
                                .engine
                                .state
                                .set_message(format!("Annotation saved (disk write: {})", e));
                        }
                    }
                } else {
                    // No git root — keep in memory only
                    state.annotations.push(annotation);
                    editor
                        .engine
                        .state
                        .set_message("Annotation saved (memory only)");
                }
            }
            state.comment_buffer.clear();
            state.comment_cursor = 0;
            state.sub_mode = ReviewSubMode::DiffView;
            editor.needs_render = true;
        }
        KeyCode::Backspace => {
            if state.comment_cursor > 0 {
                let pos = state.comment_cursor;
                // Find the previous char boundary
                let mut new_pos = pos.saturating_sub(1);
                while !state.comment_buffer.is_char_boundary(new_pos) {
                    new_pos = new_pos.saturating_sub(1);
                }
                state.comment_buffer.drain(new_pos..pos);
                state.comment_cursor = new_pos;
            }
            editor.needs_render = true;
        }
        KeyCode::Left => {
            if state.comment_cursor > 0 {
                let mut new_pos = state.comment_cursor.saturating_sub(1);
                while !state.comment_buffer.is_char_boundary(new_pos) {
                    new_pos = new_pos.saturating_sub(1);
                }
                state.comment_cursor = new_pos;
            }
            editor.needs_render = true;
        }
        KeyCode::Right => {
            let len = state.comment_buffer.len();
            if state.comment_cursor < len {
                let mut new_pos = state.comment_cursor + 1;
                while new_pos < len && !state.comment_buffer.is_char_boundary(new_pos) {
                    new_pos += 1;
                }
                state.comment_cursor = new_pos;
            }
            editor.needs_render = true;
        }
        KeyCode::Home => {
            state.comment_cursor = 0;
            editor.needs_render = true;
        }
        KeyCode::End => {
            state.comment_cursor = state.comment_buffer.len();
            editor.needs_render = true;
        }
        KeyCode::Delete => {
            let len = state.comment_buffer.len();
            if state.comment_cursor < len {
                let mut end = state.comment_cursor + 1;
                while end < len && !state.comment_buffer.is_char_boundary(end) {
                    end += 1;
                }
                state.comment_buffer.drain(state.comment_cursor..end);
            }
            editor.needs_render = true;
        }
        KeyCode::Char(ch) => {
            state.comment_buffer.insert(state.comment_cursor, ch);
            state.comment_cursor += ch.len_utf8();
            editor.needs_render = true;
        }
        _ => {}
    }
}
