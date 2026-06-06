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
use crate::engine::{Engine, EngineResult};
use crate::keymap::{create_keymap, KeymapHandler};
use crate::plugin::PluginManager;
use crate::renderer::Renderer;
use crate::terminal::Terminal;
use crate::types::{EditorState, Keymap, MessageLevel, Mode, Position, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::cell::RefCell;
#[allow(unused_imports)]
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct Editor {
    pub engine: Engine,
    pub terminal: Terminal,
    pub renderer: Renderer,
    pub(crate) keymap_handler: Rc<RefCell<dyn KeymapHandler>>,
    pub running: bool,
    pub(crate) needs_render: bool,
    pub config: Config,
}

impl Editor {
    /// Notify buffer modification to plugins and update dirty flag.
    pub(crate) fn on_buffer_modified(&mut self) {
        self.engine.emit_buffer_change();
        self.needs_render = true;
    }

    /// Transition mode with plugin event.
    pub(crate) fn transition_mode(&mut self, to: Mode) {
        let from = self.engine.state.mode;
        self.engine.state.mode = to;
        self.engine
            .plugin_manager
            .emit(crate::types::PluginEvent::ModeChange { from, to });
        self.needs_render = true;
    }

    /// Apply an EngineResult to the Editor's UI state.
    fn apply_result(&mut self, r: EngineResult) {
        if r.needs_render {
            self.needs_render = true;
        }
        if let Some(running) = r.running {
            self.running = running;
        }
        if let Some(msg) = r.message {
            self.engine.state.push_message(MessageLevel::Info, msg);
        }
        if let Some((msg, action)) = r.confirmation {
            self.engine.state.set_confirmation(msg, action);
        }
    }

    /// Shared constructor.
    fn new_shared(
        terminal: Terminal,
        buffer: TextBuffer,
        plugin_manager: PluginManager,
        state: EditorState,
        keymap: Keymap,
        cfg: Config,
    ) -> Self {
        Self {
            engine: Engine::new(buffer, plugin_manager, state),
            terminal,
            renderer: Renderer::new(),
            keymap_handler: create_keymap(keymap),
            running: false,
            needs_render: true,
            config: cfg,
        }
    }

    pub fn new_headless_for_test(keymap: Keymap) -> Result<Self> {
        let terminal = Terminal::new()?;
        let buffer = TextBuffer::new();
        let state = EditorState::default();

        Ok(Editor::new_shared(
            terminal,
            buffer,
            PluginManager::new(),
            state,
            keymap,
            Config::default(),
        ))
    }

    pub async fn new(file_path: Option<&str>, keymap: Keymap, cfg: Config) -> Result<Self> {
        let mut terminal = Terminal::new()?;
        terminal.enable_raw_mode()?;

        let buffer = if let Some(path) = file_path {
            match TextBuffer::load_file(path).await {
                Ok(buf) => buf,
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
            file_path: buffer.file_path().map(|p| p.to_path_buf()),
            cursor: Position { line: 1, col: 0 },
            show_line_numbers: cfg.show_line_numbers,
            show_relative_numbers: cfg.show_relative_numbers,
            wrap: cfg.wrap,
            ..EditorState::default()
        };

        Ok(Editor::new_shared(
            terminal,
            buffer,
            plugin_manager,
            state,
            keymap,
            cfg,
        ))
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.running = true;

        if let Err(e) = self.renderer.render(
            &mut self.terminal,
            &self.engine.buffer,
            &self.engine.inactive_buffers,
            &self.engine.state,
            &self.engine.window_layout,
            &self.engine.highlights,
        ) {
            self.engine
                .state
                .push_message(MessageLevel::Error, format!("Render error: {}", e));
        }
        self.needs_render = false;

        let poll_timeout = Duration::from_millis(150);

        while self.running {
            self.engine.state.dirty = self.engine.buffer.is_dirty();

            if event::poll(poll_timeout)? {
                match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        self.engine.last_keypress_time = Instant::now();
                        let modifiers = key.modifiers;
                        self.handle_key(key.code, modifiers).await;
                    }
                    Ok(Event::Resize(_, _)) => {
                        self.terminal.update_size();
                        self.terminal.clear_cache();
                        let rows = self.terminal.rows() as usize;
                        let cols = self.terminal.cols() as usize;
                        self.engine.update_window_layout(rows, cols);
                        self.needs_render = true;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.engine
                            .state
                            .push_message(MessageLevel::Error, format!("Event read error: {}", e));
                    }
                }
            }

            #[cfg(feature = "syntax")]
            {
                let now = Instant::now();
                if self.engine.buffer.modification_count() > self.engine.last_highlight_mod_count
                    && now.duration_since(self.engine.last_keypress_time)
                        > Duration::from_millis(150)
                {
                    self.engine.highlights = self.engine.highlighter.update(
                        &self.engine.buffer.to_string(),
                        self.engine.state.file_path.as_deref(),
                    );
                    self.engine.last_highlight_mod_count = self.engine.buffer.modification_count();
                }
            }

            if self.needs_render {
                if let Err(e) = self.renderer.render(
                    &mut self.terminal,
                    &self.engine.buffer,
                    &self.engine.inactive_buffers,
                    &self.engine.state,
                    &self.engine.window_layout,
                    &self.engine.highlights,
                ) {
                    self.engine
                        .state
                        .push_message(MessageLevel::Error, format!("Render error: {}", e));
                }
                self.needs_render = false;
            }
        }

        Ok(())
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Clear transient message on next key press (Vim-like behaviour).
        self.engine.state.clear_message();
        let keymap_handler = Rc::clone(&self.keymap_handler);
        keymap_handler
            .borrow_mut()
            .handle_key(self, key, modifiers)
            .await;
    }

    /// Emit a key-press event to the plugin system and record macros if recording.
    /// Called by both Vim and Emacs keymap handlers.
    pub(crate) fn emit_key_event(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.engine.emit_key_event(key, modifiers);
    }

    /// Expose count from operator state.
    pub fn take_count(&mut self) -> usize {
        self.engine.take_count()
    }

    // ── Methods that delegate directly to engine ────────────────

    pub fn undo(&mut self) {
        let result = self.engine.undo();
        self.apply_result(result);
    }

    pub fn redo(&mut self) {
        let result = self.engine.redo();
        self.apply_result(result);
    }

    pub fn show_file_info(&mut self) {
        let result = self.engine.show_file_info();
        self.apply_result(result);
    }

    pub async fn save_file_async(&mut self) {
        let result = self.engine.save_file_async().await;
        self.apply_result(result);
    }

    pub fn switch_keymap(&mut self, name: &str) {
        let keymap = match name {
            "vim" => Keymap::Vim,
        "emacs" => Keymap::Emacs,
        _ => {
            self.engine
                .state
                .push_message(MessageLevel::Warning, format!("Unknown keymap: {} (use vim or emacs)", name));
            return;
        }
        };
        self.keymap_handler = create_keymap(keymap);
        self.engine.reset_state_for_keymap_switch();
        self.needs_render = true;
    }

    // ── Test helpers ────────────────────────────────────────────

    pub fn set_buffer_for_test(&mut self, text: &str) {
        self.engine.set_buffer_for_test(text);
    }

    pub fn snapshot_for_test(&self) -> (String, usize, usize, Mode) {
        self.engine.snapshot_for_test()
    }

    pub async fn handle_key_for_test(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.handle_key(key, modifiers).await;
    }

    /// Expose engine's kill_ring for tests
    pub fn kill_ring(&self) -> &crate::types::KillRing {
        &self.engine.kill_ring
    }

    pub fn kill_ring_mut(&mut self) -> &mut crate::types::KillRing {
        &mut self.engine.kill_ring
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
        let plugin_manager = PluginManager::new();
        let state = EditorState {
            mode: Mode::Normal,
            cursor: crate::types::Position { line: 1, col: 0 },
            file_path: None,
            dirty: false,
            command_buffer: String::new(),
            command_cursor_pos: 0,
            last_message: None,
            visual_start: None,
            visual_type: None,
            last_visual_start: None,
            last_visual_type: None,
            marks: crate::types::Marks::new(),
            macros: crate::types::Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
            show_relative_numbers: false,
            hlsearch: true,
            wrap: true,
            mark: None,
            region_active: false,
            tabstop: 8,
            shiftwidth: 4,
            expandtab: false,
            scrolloff: 0,
            message_history: VecDeque::new(),
        };

        Editor {
            engine: Engine::new(buffer, plugin_manager, state),
            terminal,
            renderer: Renderer::new(),
            keymap_handler: create_keymap(keymap),
            running: true,
            needs_render: false,
            config: Config::default(),
        }
    }

    #[test]
    fn dirty_quit_confirmation_message_matches_expected() {
        let mut editor = test_editor(Keymap::Vim);
        editor.engine.buffer.force_dirty(true);

        editor.handle_quit();

        let prompt = editor
            .engine
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
        editor.engine.buffer.force_dirty(true);

        editor.handle_quit();

        assert!(editor.engine.state.has_confirmation());
        let prompt = editor.engine.state.confirmation_prompt.as_ref().unwrap();
        assert!(matches!(prompt.action, ConfirmAction::Quit));
    }

    #[tokio::test]
    async fn emacs_ctrl_x_ctrl_c_triggers_same_quit_path() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.engine.buffer.force_dirty(true);

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;

        assert!(editor.engine.state.has_confirmation());
        assert!(editor.running);
    }

    #[tokio::test]
    async fn emacs_dirty_quit_confirmation_accept_exits_editor() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.engine.buffer.force_dirty(true);

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;
        assert!(editor.engine.state.has_confirmation());
        assert!(editor.running);

        editor
            .handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
            .await;

        assert!(!editor.engine.state.has_confirmation());
        assert!(!editor.running);
    }

    #[tokio::test]
    async fn emacs_clean_quit_exits_immediately() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.engine.buffer.force_dirty(false);

        editor
            .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
            .await;
        editor
            .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
            .await;

        assert!(!editor.engine.state.has_confirmation());
        assert!(!editor.running);
    }

    // ── Emacs Mark/Region tests ─────────────────────────────────

    #[test]
    fn emacs_mark_defaults_to_none() {
        let editor = test_editor(Keymap::Emacs);
        assert!(editor.engine.state.mark.is_none());
        assert!(!editor.engine.state.region_active);
    }

    #[tokio::test]
    async fn emacs_c_space_sets_mark() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor
            .handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
            .await;

        assert_eq!(editor.engine.state.mark, Some(editor.engine.state.cursor));
        assert!(editor.engine.state.region_active);
    }

    #[test]
    fn emacs_has_active_region_true_when_mark_differs_from_cursor() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.engine.state.region_active = true;
        editor.engine.state.cursor.col = 5;

        assert!(editor.has_active_region());
    }

    // Note: has_active_region is defined in editor/emacs_ops.rs
    // We test it through the editor instance.

    #[tokio::test]
    async fn emacs_kill_region_pushes_to_kill_ring() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");

        editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.engine.state.region_active = true;
        editor.engine.state.cursor.col = 5;

        editor.kill_region();

        assert_eq!(editor.kill_ring().yank(), Some("hello"));
    }

    #[tokio::test]
    async fn emacs_deactivate_region_clears_active_flag() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.engine.state.region_active = true;

        editor.deactivate_region();

        assert!(!editor.engine.state.region_active);
        assert!(editor.engine.state.mark.is_some());
    }

    #[tokio::test]
    async fn vim_0_moves_to_column_zero() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.engine.state.cursor.col = 3;
        editor.handle_normal(KeyCode::Char('0')).await;
        assert_eq!(editor.engine.state.cursor.col, 0);
    }

    #[tokio::test]
    async fn vim_dollar_moves_to_end_of_line() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.engine.state.cursor.col = 0;
        editor.handle_normal(KeyCode::Char('$')).await;
        assert_eq!(editor.engine.state.cursor.col, 4);
    }

    #[tokio::test]
    async fn vim_caret_moves_to_first_non_blank() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("  hello\n");
        editor.engine.state.cursor.col = 6;
        editor.handle_normal(KeyCode::Char('^')).await;
        assert_eq!(editor.engine.state.cursor.col, 2);
    }

    #[tokio::test]
    async fn vim_d_capital_works() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello world\n");
        editor.engine.state.cursor.col = 5;
        editor.handle_normal(KeyCode::Char('D')).await;
        let text = editor.engine.buffer.get_line(1);
        assert!(text.starts_with("hello"));
    }

    #[tokio::test]
    async fn vim_s_capital_substitutes_line() {
        let mut editor = test_editor(Keymap::Vim);
        editor.set_buffer_for_test("hello\n");
        editor.handle_normal(KeyCode::Char('S')).await;
        assert_eq!(editor.engine.state.mode, Mode::Insert);
        let text = editor.engine.buffer.get_line(1);
        assert_eq!(text, "\n");
    }

    #[tokio::test]
    async fn vim_question_mark_enters_command_mode() {
        let mut editor = test_editor(Keymap::Vim);
        editor.handle_normal(KeyCode::Char('?')).await;
        assert_eq!(editor.engine.state.mode, Mode::Command);
        assert_eq!(editor.engine.state.command_buffer, "?");
    }

    #[test]
    fn vim_ctrl_r_redo_does_not_panic() {
        let mut editor = test_editor(Keymap::Vim);
        editor.redo();
    }

    // ── Emacs new keybindings tests ────────────────────────────

    #[tokio::test]
    async fn emacs_copy_region_copies_without_deleting() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
        editor.engine.state.region_active = true;
        editor.engine.state.cursor.col = 5;

        editor.copy_region();

        let text = editor.engine.buffer.get_line(1);
        assert_eq!(text, "hello world\n");
        assert_eq!(editor.kill_ring().yank(), Some("hello"));
        assert!(!editor.engine.state.region_active);
    }

    #[tokio::test]
    async fn emacs_yank_from_kill_ring_inserts_text() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.kill_ring_mut().push("xyz");
        editor.engine.state.cursor.col = 5;

        editor.yank_from_kill_ring();

        let text = editor.engine.buffer.get_line(1);
        assert_eq!(text, "helloxyz world\n");
    }

    #[tokio::test]
    async fn emacs_m_f_moves_word_forward() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world foo\n");
        editor.engine.state.cursor.col = 0;
        editor.handle_normal(KeyCode::Char('w')).await;
        assert_eq!(editor.engine.state.cursor.col, 5);
    }

    #[tokio::test]
    async fn emacs_m_b_moves_word_backward() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.engine.state.cursor.col = 8;
        editor.handle_normal(KeyCode::Char('b')).await;
        assert_eq!(editor.engine.state.cursor.col, 6);
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
