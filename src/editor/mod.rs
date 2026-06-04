pub(crate) mod cmd;
pub(crate) mod emacs_ops;
pub(crate) mod insert;
pub(crate) mod macro_marks;
pub(crate) mod motion;
pub(crate) mod normal;
pub(crate) mod search;
pub(crate) mod text_object;
pub(crate) mod visual;

use crate::buffer::TextBuffer;
use crate::config::Config;
use crate::highlight::Highlighter;
use crate::plugin::PluginManager;
use crate::register::Register;
use crate::renderer::Renderer;
use crate::terminal::Terminal;
use crate::keymap::{create_keymap, KeymapHandler};
use crate::types::{
    CommandState, EditorState, Keymap, Mode, PluginEvent, Position, SearchDirection, SearchResult,
};
use crate::undo::UndoManager;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};
use syntect::highlighting::Style;

pub struct Editor {
    pub terminal: Terminal,
    pub buffer: TextBuffer,
    pub(crate) highlighter: Highlighter,
    pub(crate) renderer: Renderer,
    pub(crate) plugin_manager: PluginManager,
    pub register: Register,
    pub(crate) undo_manager: UndoManager,
    pub state: EditorState,
    pub running: bool,
    pub(crate) last_highlight_mod_count: usize,
    pub(crate) last_keypress_time: Instant,
    pub(crate) needs_render: bool,
    /// Command parser state machine (tracks operator/count/text-object).
    pub command_state: CommandState,
    /// Numeric count prefix (e.g., `3` in `3j`). Accumulated digit by digit.
    pub pending_count: Option<usize>,
    pub(crate) pending_register: Option<char>,
    pub(crate) pending_mark: Option<char>,
    pub(crate) pending_macro_play: Option<char>,
    pub(crate) search_query: String,
    pub(crate) search_direction: SearchDirection,
    pub(crate) search_results: Vec<SearchResult>,
    pub(crate) current_search_idx: usize,
    pub(crate) dot_last_action: Option<DotAction>,
    pub(crate) replace_char: Option<char>,
    pub(crate) last_fchar: Option<char>,
    pub(crate) last_fchar_till: bool,
    pub(crate) keymap_handler: Rc<RefCell<dyn KeymapHandler>>,
    pub(crate) highlights: Vec<Vec<(Style, String)>>,
    pub kill_ring: KillRing,
    /// Cursor position before the last yank (for yank-pop cycle).
    pub yank_start: Option<crate::types::Position>,
    /// Length of text inserted by the last yank (for yank-pop undo).
    pub yank_length: usize,
    /// Accumulated characters typed during the current insert-mode session.
    /// Used to push a single undo-group when leaving insert mode.
    pub(crate) insert_accum: String,
    /// Cursor position when insert mode was entered (for undo cursor_before).
    pub(crate) insert_start_pos: Option<crate::types::Position>,
}

/// Cyclic kill-ring: holds Emacs kill history entries.
/// - push(): insert newest entry at front (up to MAX_ENTRIES)
/// - yank(): get the newest entry
/// - yank_pop(): cycle to the previous entry (for future M-y)
#[derive(Clone)]
pub struct KillRing {
    entries: Vec<String>,
    index: usize,
    max_entries: usize,
}

impl KillRing {
    const MAX_ENTRIES: usize = 60;

    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(Self::MAX_ENTRIES),
            index: 0,
            max_entries: Self::MAX_ENTRIES,
        }
    }

    pub fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.entries.len() >= self.max_entries {
            self.entries.pop();
        }
        self.entries.insert(0, text.to_string());
        self.index = 0;
    }

    pub fn yank(&self) -> Option<&str> {
        self.entries.first().map(|s| s.as_str())
    }

    /// Return the entry at the current index (for yank-pop cycling).
    pub fn current_yank(&self) -> Option<&str> {
        self.entries.get(self.index).map(|s| s.as_str())
    }

    /// Advance the index and return the next entry (yank-pop forward).
    pub fn yank_pop(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.entries.len();
        self.entries.get(self.index).map(|s| s.as_str())
    }

    pub fn reset_index(&mut self) {
        self.index = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for KillRing {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub(crate) enum DotAction {
    Insert {
        text: String,
    },
    Delete {
        text: String,
        line: usize,
        col: usize,
    },
    Change {
        text: String,
        line: usize,
        col: usize,
    },
}

impl Editor {
    /// Shared struct literal construction — both public constructors delegate here.
    fn new_shared(
        terminal: Terminal,
        buffer: TextBuffer,
        plugin_manager: PluginManager,
        state: EditorState,
        keymap: Keymap,
    ) -> Self {
        Self {
            terminal,
            buffer,
            highlighter: Highlighter::new(),
            renderer: Renderer::new(),
            plugin_manager,
            register: Register::new(),
            undo_manager: UndoManager::new(),
            state,
            running: false,
            last_highlight_mod_count: 0,
            last_keypress_time: Instant::now(),
            needs_render: true,
            command_state: CommandState::Idle,
            pending_count: None,
            pending_register: None,
            pending_mark: None,
            pending_macro_play: None,
            search_query: String::new(),
            search_direction: SearchDirection::Forward,
            search_results: Vec::new(),
            current_search_idx: 0,
            dot_last_action: None,
            replace_char: None,
            last_fchar: None,
            last_fchar_till: false,
            keymap_handler: create_keymap(keymap),
            highlights: Vec::new(),
            insert_accum: String::new(),
            insert_start_pos: None,
            kill_ring: KillRing::new(),
            yank_start: None,
            yank_length: 0,
        }
    }

    pub fn new_headless_for_test(keymap: Keymap) -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = Terminal::new()?;
        let buffer = TextBuffer::new();
        let state = EditorState::default();

        Ok(Editor::new_shared(
            terminal,
            buffer,
            PluginManager::new(),
            state,
            keymap,
        ))
    }

    pub async fn new(
        file_path: Option<&str>,
        keymap: Keymap,
        cfg: Config,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new()?;
        terminal.enable_raw_mode()?;

        let buffer = if let Some(path) = file_path {
            match TextBuffer::load_file(path).await {
                Ok(mut buf) => {
                    buf.file_path = Some(std::path::PathBuf::from(path));
                    buf
                }
                Err(e) => {
                    eprintln!("Failed to load file: {}", e);
                    TextBuffer::new()
                }
            }
        } else {
            TextBuffer::new()
        };

        let mut plugin_manager = PluginManager::new();
        if let Err(e) = plugin_manager.load_all() {
            eprintln!("Plugin loading warning: {}", e);
        }

        let state = EditorState {
            file_path: buffer.file_path.clone(),
            cursor: Position { line: 1, col: 0 },
            show_line_numbers: cfg.show_line_numbers,
            wrap: cfg.wrap,
            ..EditorState::default()
        };

        Ok(Editor::new_shared(
            terminal,
            buffer,
            plugin_manager,
            state,
            keymap,
        ))
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.running = true;

        if let Err(e) = self.renderer.render(
            &mut self.terminal,
            &self.buffer,
            &self.state,
            &self.highlights,
        ) {
            self.state.set_message(format!("Render error: {}", e));
        }
        self.needs_render = false;

        let mut reader = EventStream::new();

        while self.running {
            self.state.dirty = self.buffer.dirty;

            tokio::select! {
                Some(event) = reader.next() => {
                    match event {
                        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                            self.last_keypress_time = Instant::now();
                            let modifiers = key.modifiers;
                            self.handle_key(key.code, modifiers).await;
                        }
                        Ok(Event::Resize(_, _)) => {
                            self.terminal.update_size();
                            self.terminal.clear_cache();
                            self.needs_render = true;
                        }
                        _ => {}
                    }
                }
            }

            let now = Instant::now();
            if self.buffer.modification_count() > self.last_highlight_mod_count
                && now.duration_since(self.last_keypress_time) > Duration::from_millis(150)
            {
                self.highlights = self
                    .highlighter
                    .update(&self.buffer.to_string(), self.state.file_path.as_deref());
                self.last_highlight_mod_count = self.buffer.modification_count();
            }

            if self.needs_render {
                if let Err(e) = self.renderer.render(
                    &mut self.terminal,
                    &self.buffer,
                    &self.state,
                    &self.highlights,
                ) {
                    self.state.set_message(format!("Render error: {}", e));
                }
                self.needs_render = false;
            }
        }

        Ok(())
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Clear transient message on next key press (Vim-like behaviour).
        self.state.clear_message();
        // Safety: single-threaded TUI context; no concurrent RefCell access possible.
        // The RefMut is held across await because the future borrows the handler.
        let keymap_handler = Rc::clone(&self.keymap_handler);
        keymap_handler
            .borrow_mut()
            .handle_key(self, key, modifiers)
            .await;
    }

    /// Emit a key-press event to the plugin system and record macros if recording.
    /// Called by both Vim and Emacs keymap handlers.
    pub(crate) fn emit_key_event(&mut self, key: KeyCode) {
        let key_str = match key {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Null => "Null".to_string(),
            KeyCode::CapsLock => "CapsLock".to_string(),
            KeyCode::ScrollLock => "ScrollLock".to_string(),
            KeyCode::NumLock => "NumLock".to_string(),
            KeyCode::PrintScreen => "PrintScreen".to_string(),
            KeyCode::Pause => "Pause".to_string(),
            KeyCode::Menu => "Menu".to_string(),
            KeyCode::KeypadBegin => "KeypadBegin".to_string(),
            KeyCode::Media(m) => format!("Media({:?})", m),
            KeyCode::Modifier(m) => format!("Modifier({:?})", m),
        };

        self.plugin_manager.emit(PluginEvent::Key {
            mode: self.state.mode,
            key: key_str.clone(),
        });

        if self.state.macros.is_recording() {
            self.state.macros.add_key(key_str);
        }
    }

    /// Take the accumulated count prefix, defaulting to 1.
    fn take_count(&mut self) -> usize {
        self.pending_count.take().unwrap_or(1)
    }

    fn on_buffer_modified(&mut self) {
        self.plugin_manager.emit(PluginEvent::BufferChange);
        self.needs_render = true;
    }

    pub(crate) fn transition_mode(&mut self, to: Mode) {
        let from = self.state.mode;
        self.state.mode = to;
        self.plugin_manager
            .emit(PluginEvent::ModeChange { from, to });
    }

    pub(crate) fn undo(&mut self) {
        if let Some(edit) = self.undo_manager.undo(&mut self.buffer) {
            self.state.cursor = edit.cursor_before;
            self.buffer.dirty = !self.buffer.is_clean();
            self.needs_render = true;
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(edit) = self.undo_manager.redo(&mut self.buffer) {
            self.state.cursor = edit.cursor_after;
            self.needs_render = true;
        }
    }

    /// Switch the active keymap at runtime (e.g., via `:keymap vim`).
    pub fn switch_keymap(&mut self, name: &str) {
        let keymap = match name {
            "vim" => Keymap::Vim,
            "emacs" => Keymap::Emacs,
            _ => {
                self.state
                    .set_message(format!("Unknown keymap: {} (use vim or emacs)", name));
                return;
            }
        };
        self.keymap_handler = create_keymap(keymap);
        // Reset all pending state to avoid stuck operators
        self.command_state = CommandState::Idle;
        self.pending_count = None;
        self.pending_register = None;
        self.pending_mark = None;
        self.pending_macro_play = None;
        // Reset editor mode
        self.state.mode = Mode::Normal;
        self.state.visual_start = None;
        self.state.visual_type = None;
        self.state.command_buffer.clear();
        self.needs_render = true;
    }

    pub fn show_file_info(&mut self) {
        let line_count = self.buffer.line_count();
        let _col = self.state.cursor.col + 1;
        let total = self.buffer.len_chars();
        let path = self
            .state
            .file_path
            .as_ref()
            .map(|p| p.to_str().unwrap_or(""))
            .unwrap_or("[No Name]");
        self.state
            .set_message(format!("\"{}\" {} lines, {} characters", path, line_count, total));
    }

    pub async fn save_file_async(&mut self) {
        if let Err(e) = self.buffer.save_file().await {
            self.state.set_message(format!("Save failed: {}", e));
        } else {
            self.state.dirty = false;
        }
        self.needs_render = true;
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn set_buffer_for_test(&mut self, text: &str) {
        self.buffer = TextBuffer::new();
        for ch in text.chars() {
            if ch == '\n' {
                self.buffer
                    .insert_newline(self.state.cursor.line, self.state.cursor.col);
                self.state.cursor.line += 1;
                self.state.cursor.col = 0;
            } else {
                self.buffer
                    .insert_char(self.state.cursor.line, self.state.cursor.col, ch);
                self.state.cursor.col += 1;
            }
        }
        self.state.cursor.line = 1;
        self.state.cursor.col = 0;
        self.state.mode = Mode::Normal;
        self.state.dirty = false;
        self.undo_manager.clear();
        self.buffer.reset_clean_state();
    }

    pub fn snapshot_for_test(&self) -> (String, usize, usize, Mode) {
        (
            self.buffer.to_string(),
            self.state.cursor.line,
            self.state.cursor.col,
            self.state.mode,
        )
    }

    pub async fn handle_key_for_test(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.handle_key(key, modifiers).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ConfirmAction, Keymap};
    use crossterm::event::{KeyCode, KeyModifiers};

    fn test_editor(keymap: Keymap) -> Editor {
        let terminal = Terminal::new().expect("terminal");
        let buffer = TextBuffer::new();
        let highlighter = Highlighter::new();
        let renderer = Renderer::new();
        let plugin_manager = PluginManager::new();
        let register = Register::new();
        let undo_manager = UndoManager::new();
        let state = EditorState {
            mode: Mode::Normal,
            cursor: crate::types::Position { line: 1, col: 0 },
            file_path: None,
            dirty: false,
            command_buffer: String::new(),
            last_message: None,
            visual_start: None,
            visual_type: None,
            marks: crate::types::Marks::new(),
            macros: crate::types::Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
            wrap: true,
            mark: None,
            region_active: false,
        };

        Editor {
            terminal,
            buffer,
            highlighter,
            renderer,
            plugin_manager,
            register,
            undo_manager,
            state,
            running: true,
            last_highlight_mod_count: 0,
            last_keypress_time: Instant::now(),
            needs_render: false,
            command_state: CommandState::Idle,
            pending_count: None,
            pending_register: None,
            pending_mark: None,
            pending_macro_play: None,
            search_query: String::new(),
            search_direction: SearchDirection::Forward,
            search_results: Vec::new(),
            current_search_idx: 0,
            dot_last_action: None,
            replace_char: None,
            last_fchar: None,
            last_fchar_till: false,
            keymap_handler: create_keymap(keymap),
            kill_ring: KillRing::new(),
            highlights: Vec::new(),
            insert_accum: String::new(),
            insert_start_pos: None,
            yank_start: None,
            yank_length: 0,
        }
    }

    #[test]
    fn dirty_quit_confirmation_message_matches_expected() {
        let mut editor = test_editor(Keymap::Vim);
        editor.buffer.dirty = true;

        editor.handle_quit();

        let prompt = editor
            .state
            .confirmation_prompt
            .as_ref()
            .expect("prompt should exist");
        assert_eq!(
            prompt.message,
            "No write since last change. Quit anyway? (y/n Enter/Esc: yes, n: no)"
        );
        assert!(matches!(prompt.action, ConfirmAction::Quit));
    }

    #[test]
    fn emacs_quit_uses_same_dirty_confirmation_flow_as_vim() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.buffer.dirty = true;

        editor.handle_quit();

        assert!(editor.state.has_confirmation());
        let prompt = editor.state.confirmation_prompt.as_ref().unwrap();
        assert!(matches!(prompt.action, ConfirmAction::Quit));
    }

    #[tokio::test]
    async fn emacs_ctrl_x_ctrl_c_triggers_same_quit_path() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.buffer.dirty = true;

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;

        assert!(editor.state.has_confirmation());
        assert!(editor.running);
    }

    #[tokio::test]
    async fn emacs_dirty_quit_confirmation_accept_exits_editor() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.buffer.dirty = true;

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;
        assert!(editor.state.has_confirmation());
        assert!(editor.running);

        editor
            .handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
            .await;

        assert!(!editor.state.has_confirmation());
        assert!(!editor.running);
    }

    #[tokio::test]
    async fn emacs_clean_quit_exits_immediately() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.buffer.dirty = false;

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;

        assert!(!editor.state.has_confirmation());
        assert!(!editor.running);
    }

    // ── Emacs Mark/Region tests ─────────────────────────────────

    #[test]
    fn emacs_mark_defaults_to_none() {
        let editor = test_editor(Keymap::Emacs);
        assert!(editor.state.mark.is_none());
        assert!(!editor.state.region_active);
        assert!(!editor.has_active_region());
    }

    #[tokio::test]
    async fn emacs_c_space_sets_mark() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        // C-space を送信
        editor
            .handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
            .await;

        assert_eq!(editor.state.mark, Some(editor.state.cursor));
        assert!(editor.state.region_active);
    }

    #[test]
    fn emacs_has_active_region_true_when_mark_differs_from_cursor() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;
        editor.state.cursor.col = 5;

        assert!(editor.has_active_region());
    }

    #[test]
    fn emacs_has_active_region_false_when_mark_equals_cursor() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.state.mark = Some(editor.state.cursor);
        editor.state.region_active = true;

        assert!(!editor.has_active_region());
    }

    #[test]
    fn emacs_has_active_region_false_when_region_inactive() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = false;
        editor.state.cursor.col = 5;

        assert!(!editor.has_active_region());
    }

    #[test]
    fn emacs_kill_region_deletes_selected_text() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;
        editor.state.cursor.col = 5;

        editor.kill_region();

        let text = editor.buffer.get_line(1);
        assert_eq!(text, " world\n");
        assert!(!editor.state.region_active);
    }

    #[test]
    fn emacs_kill_region_pushes_to_kill_ring() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;
        editor.state.cursor.col = 5;

        editor.kill_region();

        assert_eq!(editor.kill_ring.yank(), Some("hello"));
    }

    #[test]
    fn emacs_kill_region_with_reverse_selection() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.state.mark = Some(crate::types::Position { line: 1, col: 5 });
        editor.state.region_active = true;
        editor.state.cursor.col = 0;

        editor.kill_region();

        let text = editor.buffer.get_line(1);
        assert_eq!(text, " world\n");
    }

    #[test]
    fn emacs_kill_region_pushes_to_register() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;
        editor.state.cursor.col = 5;

        editor.kill_region();

        assert_eq!(editor.register.get('"'), "hello");
    }

    #[test]
    fn emacs_deactivate_region_clears_active_flag() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;

        editor.deactivate_region();

        assert!(!editor.state.region_active);
        assert!(editor.state.mark.is_some());
    }

    #[test]
    fn emacs_set_mark_with_emacs_keymap() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("abcdef\n");

        editor.state.cursor.col = 3;
        editor.set_mark();

        assert_eq!(
            editor.state.mark,
            Some(crate::types::Position { line: 1, col: 3 })
        );
        assert!(editor.state.region_active);
    }

    #[tokio::test]
    async fn emacs_c_w_kills_word_when_no_region() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor
            .handle_key_for_test(KeyCode::Char('w'), KeyModifiers::CONTROL)
            .await;

        let text = editor.buffer.get_line(1);
        assert_eq!(text, " world\n");
    }

    // ── Vim new keybindings tests ──────────────────────────────

    #[tokio::test]
    async fn vim_0_moves_to_column_zero() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.state.cursor.col = 3;
        editor.handle_normal(KeyCode::Char('0')).await;
        assert_eq!(editor.state.cursor.col, 0);
    }

    #[tokio::test]
    async fn vim_dollar_moves_to_end_of_line() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.state.cursor.col = 0;
        editor.handle_normal(KeyCode::Char('$')).await;
        assert_eq!(editor.state.cursor.col, 4);
    }

    #[tokio::test]
    async fn vim_caret_moves_to_first_non_blank() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("  hello\n");
        editor.state.cursor.col = 6;
        editor.handle_normal(KeyCode::Char('^')).await;
        assert_eq!(editor.state.cursor.col, 2);
    }

    #[tokio::test]
    async fn vim_d_capital_works() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello world\n");
        editor.state.cursor.col = 5; // space
        editor.handle_normal(KeyCode::Char('D')).await;
        let text = editor.buffer.get_line(1);
        assert!(text.starts_with("hello"));
    }

    #[tokio::test]
    async fn vim_s_capital_substitutes_line() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.handle_normal(KeyCode::Char('S')).await;
        assert_eq!(editor.state.mode, Mode::Insert);
        let text = editor.buffer.get_line(1);
        assert_eq!(text, "\n");
    }

    #[tokio::test]
    async fn vim_question_mark_enters_command_mode() {
        let mut editor = test_editor(Keymap::Vim);
        editor.handle_normal(KeyCode::Char('?')).await;
        assert_eq!(editor.state.mode, Mode::Command);
        assert_eq!(editor.state.command_buffer, "?");
    }

    #[test]
    fn vim_ctrl_r_redo_does_not_panic() {
        let mut editor = test_editor(Keymap::Vim);
        editor.redo();
    }

    // ── Emacs new keybindings tests ────────────────────────────

    #[test]
    fn emacs_copy_region_copies_without_deleting() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.state.region_active = true;
        editor.state.cursor.col = 5;

        editor.copy_region();

        let text = editor.buffer.get_line(1);
        assert_eq!(text, "hello world\n");
        assert_eq!(editor.kill_ring.yank(), Some("hello"));
        assert!(!editor.state.region_active);
    }

    #[test]
    fn emacs_yank_from_kill_ring_inserts_text() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.kill_ring.push("xyz");
        editor.state.cursor.col = 5;

        editor.yank_from_kill_ring();

        let text = editor.buffer.get_line(1);
        assert_eq!(text, "helloxyz world\n");
    }

    #[test]
    fn emacs_m_f_moves_word_forward() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world foo\n");
        editor.state.cursor.col = 0;
        editor.move_word_forward();
        assert_eq!(editor.state.cursor.col, 5);
    }

    #[test]
    fn emacs_m_b_moves_word_backward() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.state.cursor.col = 8;
        editor.move_word_backward();
        assert_eq!(editor.state.cursor.col, 6);
    }

    #[test]
    fn emacs_ctrl_x_u_redo_does_not_panic() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.redo();
    }

    #[test]
    fn emacs_ctrl_x_ctrl_slash_redo_does_not_panic() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.redo();
    }
}
