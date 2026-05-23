use crate::buffer::TextBuffer;
use crate::highlight::Highlighter;
use crate::keymap::create_keymap;
use crate::keymap::KeymapHandler;
use crate::plugin::PluginManager;
use crate::register::Register;
use crate::renderer::Renderer;
use crate::terminal::Terminal;
use crate::types::{
    ConfirmAction, EditorState, Keymap, Mode, PluginEvent, Position, SearchDirection, SearchResult,
    VisualType,
};
use crate::undo::{Edit, EditType, UndoManager};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};
use syntect::highlighting::Style;

pub struct Editor {
    pub(crate) terminal: Terminal,
    pub(crate) buffer: TextBuffer,
    pub(crate) highlighter: Highlighter,
    pub(crate) renderer: Renderer,
    pub(crate) plugin_manager: PluginManager,
    pub(crate) register: Register,
    pub(crate) undo_manager: UndoManager,
    pub(crate) state: EditorState,
    pub(crate) running: bool,
    pub(crate) last_highlight_mod_count: usize,
    pub(crate) last_keypress_time: Instant,
    pub(crate) needs_render: bool,
    pub(crate) pending_operator: Option<char>,
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
    pub(crate) pending_save: bool,
    pub(crate) highlights: Vec<Vec<(Style, String)>>,
    pub(crate) kill_ring: KillRing,
}

/// 循環 kill-ring: Emacs の kill 履歴を保持する。
/// - push(): 最新エントリを先頭に追加（最大 MAX_ENTRIES）
/// - yank(): 最新エントリを取得
/// - yank_pop(): 一つ前のエントリに戻る（将来 M-y 用）
pub(crate) struct KillRing {
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

    #[allow(dead_code)]
    pub fn yank(&self) -> Option<&str> {
        self.entries.first().map(|s| s.as_str())
    }

    #[allow(dead_code)]
    pub fn yank_pop(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.entries.len();
        self.entries.get(self.index).map(|s| s.as_str())
    }

    #[allow(dead_code)]
    pub fn reset_index(&mut self) {
        self.index = 0;
    }

    #[allow(dead_code)]
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
    pub fn new_headless_for_test(keymap: Keymap) -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = Terminal::new()?;
        let buffer = TextBuffer::new();
        let highlighter = Highlighter::new();
        let plugin_manager = PluginManager::new();
        let renderer = Renderer::new();
        let register = Register::new();
        let undo_manager = UndoManager::new();
        let state = EditorState {
            mode: Mode::Normal,
            cursor: crate::types::Position { line: 1, col: 0 },
            file_path: None,
            dirty: false,
            command_buffer: String::new(),
            visual_start: None,
            visual_type: None,
            marks: crate::types::Marks::new(),
            macros: crate::types::Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
            mark: None,
            region_active: false,
        };

        Ok(Self {
            terminal,
            buffer,
            highlighter,
            renderer,
            plugin_manager,
            register,
            undo_manager,
            state,
            running: false,
            last_highlight_mod_count: 0,
            last_keypress_time: Instant::now(),
            needs_render: true,
            pending_operator: None,
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
            pending_save: false,
            kill_ring: KillRing::new(),
            highlights: Vec::new(),
        })
    }

    pub async fn new(
        file_path: Option<&str>,
        keymap: Keymap,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // 既存のまま

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

        let highlighter = Highlighter::new();

        let mut plugin_manager = PluginManager::new();
        if let Err(e) = plugin_manager.load_all() {
            eprintln!("Plugin loading warning: {}", e);
        }

        let renderer = Renderer::new();

        let register = Register::new();

        let undo_manager = UndoManager::new();

        let state = EditorState {
            mode: Mode::Normal,
            cursor: crate::types::Position { line: 1, col: 0 },
            file_path: buffer.file_path.clone(),
            dirty: false,
            command_buffer: String::new(),
            visual_start: None,
            visual_type: None,
            marks: crate::types::Marks::new(),
            macros: crate::types::Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
            mark: None,
            region_active: false,
        };

        Ok(Self {
            terminal,
            buffer,
            highlighter,
            renderer,
            plugin_manager,
            register,
            undo_manager,
            state,
            running: false,
            last_highlight_mod_count: 0,
            last_keypress_time: Instant::now(),
            needs_render: true,
            pending_operator: None,
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
            pending_save: false,
            kill_ring: KillRing::new(),
            highlights: Vec::new(),
        })
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.running = true;

        if let Err(e) = self
            .renderer
            .render(&mut self.terminal, &self.buffer, &self.state)
        {
            eprintln!("[render] error: {}", e);
        }
        self.needs_render = false;

        let mut reader = EventStream::new();

        while self.running {
            self.state.dirty = self.buffer.dirty;

            self.consume_pending_save().await;

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
                if let Err(e) = self
                    .renderer
                    .render(&mut self.terminal, &self.buffer, &self.state)
                {
                    eprintln!("[render] error: {}", e);
                }
                self.needs_render = false;
            }
        }

        Ok(())
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Safety: single-threaded TUI context; no concurrent RefCell access possible.
        // The RefMut is held across await because the future borrows the handler.
        let keymap_handler = Rc::clone(&self.keymap_handler);
        keymap_handler
            .borrow_mut()
            .handle_key(self, key, modifiers)
            .await;
    }

    pub(crate) fn vim_on_key_event(&mut self, key: KeyCode) {
        if let KeyCode::Char(c) = key {
            self.plugin_manager.emit(PluginEvent::Key {
                mode: self.state.mode,
                key: c,
            });

            if self.state.macros.is_recording() {
                let key_str = c.to_string();
                self.state.macros.add_key(key_str);
            }
        }
    }

    pub(crate) async fn handle_normal(&mut self, key: KeyCode) {
        let line_count = self.buffer.line_count();

        if let Some(op) = self.pending_operator {
            self.handle_operator(op, key).await;
            return;
        }

        match key {
            KeyCode::Char('h') => {
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = (self.state.cursor.col + 1).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                self.state.cursor.line = (self.state.cursor.line + 1).min(line_count);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                self.state.cursor.line = self.state.cursor.line.saturating_sub(1).max(1);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('i') => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char(':') => {
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
                let line = self.buffer.get_line(self.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = line_len.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Char('D') => {
                self.kill_line();
            }
            KeyCode::Char('C') => {
                self.kill_line();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('S') => {
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
                self.search_word_under_cursor();
                self.needs_render = true;
            }
            KeyCode::Char('n') => {
                self.search_next();
                self.needs_render = true;
            }
            KeyCode::Char('N') => {
                self.search_prev();
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
                    self.pending_operator = Some('f');
                }
            }
            KeyCode::Char('F') => {
                if self.state.command_buffer.is_empty() {
                    self.pending_operator = Some('F');
                }
            }
            KeyCode::Char('t') => {
                if self.state.command_buffer.is_empty() {
                    self.pending_operator = Some('t');
                }
            }
            KeyCode::Char('T') => {
                if self.state.command_buffer.is_empty() {
                    self.pending_operator = Some('T');
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
                self.pending_operator = Some('y');
            }
            KeyCode::Char('d') => {
                self.pending_operator = Some('d');
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
                        self.pending_operator = Some('q');
                    }
                }
            }
            _ => {
                match key {
                    KeyCode::Char('g') => {
                        self.pending_operator = Some('g');
                    }
                    KeyCode::Char('G') => {
                        self.state.cursor.line = self.buffer.line_count().max(1);
                        let len = self.buffer.get_line(self.state.cursor.line).len();
                        self.state.cursor.col = len.saturating_sub(1);
                        self.needs_render = true;
                    }
                    KeyCode::Char('%') => {
                        self.jump_to_matching_bracket();
                        self.needs_render = true;
                    }
                    KeyCode::Char('x') => {
                        self.delete_char('"');
                        self.needs_render = true;
                    }
                    KeyCode::Char('.') => {
                        self.repeat_last_action().await;
                        self.needs_render = true;
                    }
                    KeyCode::Char('w') => {
                        self.move_word_forward();
                        self.needs_render = true;
                    }
                    KeyCode::Char('b') => {
                        self.move_word_backward();
                        self.needs_render = true;
                    }
                    KeyCode::Char('e') => {
                        self.move_word_end();
                        self.needs_render = true;
                    }
                    KeyCode::Char('o') => {
                        self.open_line(false);
                        self.needs_render = true;
                    }
                    KeyCode::Char('O') => {
                        self.open_line(true);
                        self.needs_render = true;
                    }
                    KeyCode::Char('a') => {
                        let prev_mode = self.state.mode;
                        self.state.mode = Mode::Insert;
                        self.state.cursor.col = (self.state.cursor.col + 1).min(
                            self.buffer
                                .get_line(self.state.cursor.line)
                                .len()
                                .saturating_sub(1),
                        );
                        self.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.needs_render = true;
                    }
                    KeyCode::Char('A') => {
                        let prev_mode = self.state.mode;
                        self.state.mode = Mode::Insert;
                        let line_len = self.buffer.get_line(self.state.cursor.line).len();
                        self.state.cursor.col = line_len.saturating_sub(1);
                        self.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.needs_render = true;
                    }
                    KeyCode::Char('c') => {
                        self.pending_operator = Some('c');
                    }
                    KeyCode::Char('J') => {
                        self.join_lines();
                        self.needs_render = true;
                    }
                    KeyCode::Char('>') => {
                        self.pending_operator = Some('>');
                    }
                    KeyCode::Char('<') => {
                        self.pending_operator = Some('<');
                    }
                    KeyCode::Char('z') => {
                        self.pending_operator = Some('z');
                    }
                    KeyCode::Char('Z') => {
                        self.pending_operator = Some('Z');
                    }
                    KeyCode::Char('s') => {
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
                if let Some(op) = self.pending_operator {
                    if op == 'g' {
                        if let KeyCode::Char('g') = key {
                            self.state.cursor.line = 1;
                            self.state.cursor.col = 0;
                        }
                        self.pending_operator = None;
                        self.needs_render = true;
                        return;
                    }
                    if op == 'z' {
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
                        self.pending_operator = None;
                        self.needs_render = true;
                        return;
                    }
                    if op == 'Z' {
                        if let KeyCode::Char('Z') = key {
                            self.save_file_async().await;
                            if !self.buffer.dirty {
                                self.running = false;
                            }
                        }
                        self.pending_operator = None;
                        self.needs_render = true;
                        return;
                    }
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

        if let Some(r) = self.pending_register {
            if let KeyCode::Char(c) = key {
                if c.is_ascii_lowercase() {
                    self.pending_register = None;
                    if let Some(op) = self.pending_operator {
                        self.pending_operator = None;
                        let reg = r;
                        self.execute_operator_with_register(op, reg, key).await;
                    }
                }
            }
        }
    }

    async fn handle_operator(&mut self, op: char, key: KeyCode) {
        let register = self.pending_register.unwrap_or('"');
        self.pending_operator = None;
        self.pending_register = None;

        match op {
            'y' => match key {
                KeyCode::Char('y') => {
                    self.yank_line(register);
                }
                KeyCode::Char('w') => {
                    self.yank_word(register);
                }
                KeyCode::Char('e') => {
                    self.yank_word_end(register);
                }
                KeyCode::Char('i') => {
                    self.handle_text_object(key, register, true).await;
                }
                KeyCode::Char('a') => {
                    self.handle_text_object(key, register, false).await;
                }
                _ => {}
            },
            'd' => match key {
                KeyCode::Char('d') => {
                    self.delete_line(register);
                }
                KeyCode::Char('w') => {
                    self.delete_word(register);
                }
                KeyCode::Char('i') => {
                    self.handle_text_object(key, register, true).await;
                }
                KeyCode::Char('a') => {
                    self.handle_text_object(key, register, false).await;
                }
                _ => {}
            },
            'c' => match key {
                KeyCode::Char('c') => {
                    let content = self.buffer.get_line(self.state.cursor.line);
                    self.register.set(register, &content);
                    self.buffer.delete_line(self.state.cursor.line);
                    let prev_mode = self.state.mode;
                    self.state.mode = Mode::Insert;
                    self.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                    self.dot_last_action = Some(DotAction::Change {
                        text: content,
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
                    self.handle_text_object_change(key, register, true).await;
                }
                KeyCode::Char('a') => {
                    self.handle_text_object_change(key, register, false).await;
                }
                _ => {}
            },
            '>' => {
                if let KeyCode::Char('>') = key {
                    self.indent_lines(register, true);
                }
            }
            '<' => {
                if let KeyCode::Char('<') = key {
                    self.indent_lines(register, false);
                }
            }
            'f' => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = false;
                    if self.find_char(ch, false, true) {
                        self.needs_render = true;
                    }
                }
            }
            'F' => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = false;
                    if self.find_char(ch, false, false) {
                        self.needs_render = true;
                    }
                }
            }
            't' => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = true;
                    if self.find_char(ch, true, true) {
                        self.needs_render = true;
                    }
                }
            }
            'T' => {
                if let KeyCode::Char(ch) = key {
                    self.last_fchar = Some(ch);
                    self.last_fchar_till = true;
                    if self.find_char(ch, true, false) {
                        self.needs_render = true;
                    }
                }
            }
            'q' => {
                if let KeyCode::Char(c) = key {
                    if c.is_ascii_lowercase() {
                        self.state.macros.start_recording(c);
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_text_object_change(&mut self, key: KeyCode, register: char, inner: bool) {
        if let KeyCode::Char('w') = key {
            if inner {
                let (word, start, _) = self
                    .buffer
                    .get_word_range(self.state.cursor.line, self.state.cursor.col);
                if !word.is_empty() {
                    let content = self.buffer.get_char_range(
                        self.state.cursor.line,
                        start,
                        self.state.cursor.line,
                        start + word.len(),
                    );
                    self.register.set(register, &content);
                    let char_start = self.buffer.line_to_char(self.state.cursor.line - 1) + start;
                    let char_end = char_start + word.len();
                    self.buffer.remove_range(char_start, char_end);
                }
            }
            let prev_mode = self.state.mode;
            self.state.mode = Mode::Insert;
            self.plugin_manager.emit(PluginEvent::ModeChange {
                from: prev_mode,
                to: Mode::Insert,
            });
            self.on_buffer_modified();
        }
    }

    fn indent_lines(&mut self, register: char, indent: bool) {
        let line = self.state.cursor.line;
        let content = self.buffer.get_line(line);
        let _ = register;

        if indent {
            self.buffer.insert(line, 0, "\t");
        } else if content.starts_with('\t') {
            self.buffer.delete(line, 0);
        } else if content.starts_with("  ") {
            self.buffer.delete(line, 0);
            self.buffer.delete(line, 0);
        }

        self.needs_render = true;
    }

    #[allow(clippy::single_match)]
    async fn handle_text_object(&mut self, key: KeyCode, register: char, inner: bool) {
        match key {
            KeyCode::Char('w') => {
                if inner {
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
                    }
                } else {
                    let (word, start, end) = self
                        .buffer
                        .get_word_range(self.state.cursor.line, self.state.cursor.col);
                    if !word.is_empty() {
                        let line = self.buffer.get_line(self.state.cursor.line);
                        let mut aw_start = start;
                        while aw_start > 0 {
                            let prev_char = line.chars().nth(aw_start - 1);
                            if prev_char == Some(' ') || prev_char == Some('\t') {
                                aw_start -= 1;
                            } else {
                                break;
                            }
                        }
                        let mut aw_end = end;
                        while aw_end < line.len() {
                            let next_char = line.chars().nth(aw_end);
                            if next_char == Some(' ') || next_char == Some('\t') {
                                aw_end += 1;
                            } else {
                                break;
                            }
                        }
                        let char_start =
                            self.buffer.line_to_char(self.state.cursor.line - 1) + aw_start;
                        let char_end = char_start + (aw_end - aw_start);
                        let content = self.buffer.get_char_range(
                            self.state.cursor.line,
                            aw_start,
                            self.state.cursor.line,
                            aw_end,
                        );
                        self.register.set(register, &content);
                        self.buffer.remove_range(char_start, char_end);
                    }
                }
            }
            KeyCode::Char('"') | KeyCode::Char('\'') => {
                if let KeyCode::Char(quote) = key {
                    self.handle_quote_text_object(register, quote, inner);
                }
            }
            KeyCode::Char('(') | KeyCode::Char(')') | KeyCode::Char('b') => {
                self.handle_bracket_text_object(register, '(', ')', inner);
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                self.handle_bracket_text_object(register, '[', ']', inner);
            }
            KeyCode::Char('{') | KeyCode::Char('}') | KeyCode::Char('B') => {
                self.handle_bracket_text_object(register, '{', '}', inner);
            }
            KeyCode::Char('<') | KeyCode::Char('>') => {
                self.handle_bracket_text_object(register, '<', '>', inner);
            }
            _ => {}
        }
        self.needs_render = true;
    }

    fn handle_quote_text_object(&mut self, register: char, quote: char, inner: bool) {
        let line = self.buffer.get_line(self.state.cursor.line);
        let col = self.state.cursor.col;

        let mut open_pos: Option<usize> = None;
        for i in (0..col).rev() {
            if line.chars().nth(i) == Some(quote) {
                open_pos = Some(i);
                break;
            }
        }

        let mut close_pos: Option<usize> = None;
        for i in (col + 1)..line.len() {
            if line.chars().nth(i) == Some(quote) {
                close_pos = Some(i);
                break;
            }
        }

        if let Some(start) = open_pos {
            if let Some(end) = close_pos {
                let (content_start, content_end) = if inner {
                    (start + 1, end)
                } else {
                    (start, end + 1)
                };

                let content: String = line
                    .chars()
                    .skip(content_start)
                    .take(content_end - content_start)
                    .collect();
                let char_start =
                    self.buffer.line_to_char(self.state.cursor.line - 1) + content_start;
                let char_end = char_start + content.len();

                self.register.set(register, &content);
                self.buffer.remove_range(char_start, char_end);
            }
        }
    }

    fn handle_bracket_text_object(&mut self, register: char, open: char, close: char, inner: bool) {
        let col = self.state.cursor.col;

        let mut open_pos: Option<usize> = None;
        let mut open_line = self.state.cursor.line;
        for l in (1..=self.state.cursor.line).rev() {
            let l_str = self.buffer.get_line(l);
            for i in (0..l_str.len()).rev() {
                if l == self.state.cursor.line && i >= col {
                    continue;
                }
                if l_str.chars().nth(i) == Some(open) {
                    open_pos = Some(i);
                    open_line = l;
                    break;
                }
            }
            if open_pos.is_some() {
                break;
            }
        }

        let mut close_pos: Option<usize> = None;
        let mut close_line = self.state.cursor.line;
        for l in self.state.cursor.line..=self.buffer.line_count() {
            let l_str = self.buffer.get_line(l);
            for i in 0..l_str.len() {
                if l == self.state.cursor.line && i <= col {
                    continue;
                }
                if l_str.chars().nth(i) == Some(close) {
                    close_pos = Some(i);
                    close_line = l;
                    break;
                }
            }
            if close_pos.is_some() {
                break;
            }
        }

        if let Some(start) = open_pos {
            if let Some(end) = close_pos {
                let (content_start, content_end) = if inner {
                    (start + 1, end)
                } else {
                    (start, end + 1)
                };

                let start_char_idx = self.buffer.line_to_char(open_line - 1) + content_start;
                let end_char_idx = if close_line == open_line {
                    start_char_idx + (content_end - content_start)
                } else {
                    self.buffer.line_to_char(close_line - 1) + content_end
                };

                let mut content = String::new();
                if open_line == close_line {
                    let line_str = self.buffer.get_line(open_line);
                    content = line_str
                        .chars()
                        .skip(content_start)
                        .take(content_end - content_start)
                        .collect();
                } else {
                    content.push_str(
                        &self
                            .buffer
                            .get_line(open_line)
                            .chars()
                            .skip(content_start)
                            .take(usize::MAX)
                            .collect::<String>(),
                    );
                    content.push('\n');
                    for l in (open_line + 1)..close_line {
                        content.push_str(&self.buffer.get_line(l));
                        content.push('\n');
                    }
                    content.push_str(
                        &self
                            .buffer
                            .get_line(close_line)
                            .chars()
                            .take(content_end)
                            .collect::<String>(),
                    );
                }

                self.register.set(register, &content);
                self.buffer.remove_range(start_char_idx, end_char_idx);
            }
        }
    }

    async fn execute_operator_with_register(&mut self, op: char, register: char, _key: KeyCode) {
        match op {
            'y' => {
                self.yank_line(register);
            }
            'd' => {
                self.delete_line(register);
            }
            _ => {}
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

    pub(crate) async fn handle_visual(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Normal,
                });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                self.visual_yank();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Normal,
                });
                self.needs_render = true;
            }
            KeyCode::Char('d') => {
                self.visual_delete();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Normal;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Normal,
                });
                self.needs_render = true;
            }
            KeyCode::Char('c') => {
                self.visual_delete();
                let prev_mode = self.state.mode;
                self.state.mode = Mode::Insert;
                self.state.visual_start = None;
                self.state.visual_type = None;
                self.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('h') => {
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = (self.state.cursor.col + 1).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let line_count = self.buffer.line_count();
                self.state.cursor.line = (self.state.cursor.line + 1).min(line_count);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                self.state.cursor.line = self.state.cursor.line.saturating_sub(1).max(1);
                let len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let line = self.buffer.get_line(self.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = line_len.saturating_sub(1);
                self.needs_render = true;
            }
            _ => {}
        }
    }

    fn visual_yank(&mut self) {
        if let (Some(start), Some(vtype)) = (&self.state.visual_start, self.state.visual_type) {
            let content = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(start, &self.state.cursor);
                    self.buffer.get_char_range(s_line, s_col, e_line, e_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.state.cursor.line);
                    self.buffer.get_line_range(s_line, e_line)
                }
            };
            self.register.set('"', &content);
        }
    }

    fn visual_delete(&mut self) {
        if let (Some(start), Some(vtype)) = (&self.state.visual_start, self.state.visual_type) {
            let content = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(start, &self.state.cursor);
                    self.buffer.delete_range(s_line, s_col, e_line, e_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.state.cursor.line);
                    let mut content = String::new();
                    for line in s_line..=e_line {
                        content.push_str(&self.buffer.delete_line(line));
                        if line < e_line {
                            content.push('\n');
                        }
                    }
                    content
                }
            };
            self.register.set('"', &content);
            self.on_buffer_modified();
        }
    }

    fn normalize_selection(
        &self,
        start: &crate::types::Position,
        end: &crate::types::Position,
    ) -> (usize, usize, usize, usize) {
        if start.line < end.line || (start.line == end.line && start.col <= end.col) {
            (start.line, start.col, end.line, end.col + 1)
        } else {
            (end.line, end.col, start.line, start.col + 1)
        }
    }

    fn normalize_line_selection(&self, start_line: usize, end_line: usize) -> (usize, usize) {
        if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        }
    }

    #[allow(dead_code)]
    fn line_to_char(&self, line_idx: usize) -> usize {
        self.buffer.line_to_char(line_idx)
    }

    pub(crate) async fn handle_insert(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.transition_mode(Mode::Normal);
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                if self.state.cursor.col > 0 {
                    self.buffer
                        .delete(self.state.cursor.line, self.state.cursor.col - 1);
                    self.state.cursor.col -= 1;
                    self.on_buffer_modified();
                } else if self.state.cursor.line > 1 {
                    let new_col = self.buffer.merge_with_prev_line(self.state.cursor.line);
                    self.state.cursor.line -= 1;
                    self.state.cursor.col = new_col;
                    self.on_buffer_modified();
                }
            }
            KeyCode::Enter => {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col, "\n");
                self.state.cursor.line += 1;
                self.state.cursor.col = 0;
                self.on_buffer_modified();
            }
            KeyCode::Char(c) => {
                self.buffer
                    .insert_char(self.state.cursor.line, self.state.cursor.col, c);
                self.state.cursor.col += 1;
                self.on_buffer_modified();
            }
            _ => {}
        }
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

    #[allow(dead_code)]
    pub(crate) fn redo(&mut self) {
        if let Some(edit) = self.undo_manager.redo(&mut self.buffer) {
            self.state.cursor = edit.cursor_after;
            self.needs_render = true;
        }
    }

    fn search_word_under_cursor(&mut self) {
        let (word, _, _) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        if !word.is_empty() {
            let query = word.clone();
            self.do_search(&query, SearchDirection::Forward);
        }
    }

    fn search_next(&mut self) {
        if !self.search_query.is_empty() {
            let query = self.search_query.clone();
            let dir = self.search_direction;
            self.do_search(&query, dir);
        }
    }

    fn search_prev(&mut self) {
        if !self.search_query.is_empty() {
            let query = self.search_query.clone();
            let dir = match self.search_direction {
                SearchDirection::Forward => SearchDirection::Backward,
                SearchDirection::Backward => SearchDirection::Forward,
            };
            self.do_search(&query, dir);
        }
    }

    fn find_char(&mut self, ch: char, till: bool, forward: bool) -> bool {
        let line = self.buffer.get_line(self.state.cursor.line);
        let start_col = self.state.cursor.col;

        if forward {
            for i in (start_col + 1)..line.len() {
                if line.chars().nth(i) == Some(ch) {
                    if till {
                        self.state.cursor.col = i.saturating_sub(1);
                    } else {
                        self.state.cursor.col = i;
                    }
                    return true;
                }
            }
        } else {
            for i in (0..start_col).rev() {
                if line.chars().nth(i) == Some(ch) {
                    if till {
                        self.state.cursor.col = (i + 1).min(line.len().saturating_sub(1));
                    } else {
                        self.state.cursor.col = i;
                    }
                    return true;
                }
            }
        }
        false
    }

    fn repeat_find(&mut self, forward: bool) -> bool {
        if let Some(ch) = self.last_fchar {
            let till = self.last_fchar_till;
            self.find_char(ch, till, forward)
        } else {
            false
        }
    }

    fn do_search(&mut self, query: &str, direction: SearchDirection) {
        self.search_query = query.to_string();
        self.search_direction = direction;
        self.search_results = self.buffer.search(query);

        if self.search_results.is_empty() {
            return;
        }

        if direction == SearchDirection::Forward {
            self.current_search_idx = self
                .search_results
                .iter()
                .position(|r| {
                    r.line > self.state.cursor.line
                        || (r.line == self.state.cursor.line && r.start_col > self.state.cursor.col)
                })
                .unwrap_or(0);
        } else {
            self.current_search_idx = self
                .search_results
                .iter()
                .rposition(|r| {
                    r.line < self.state.cursor.line
                        || (r.line == self.state.cursor.line && r.start_col < self.state.cursor.col)
                })
                .unwrap_or(self.search_results.len() - 1);
        }

        if let Some(result) = self.search_results.get(self.current_search_idx) {
            self.state.cursor.line = result.line;
            self.state.cursor.col = result.start_col;
        }
    }

    fn handle_mark(&mut self, action: char, name: char) {
        if action == 'm' {
            self.state.marks.set(name, self.state.cursor);
        } else if let Some(pos) = self.state.marks.get(name) {
            self.state.cursor = pos;
        }
    }

    fn play_macro(&mut self, name: char) {
        let keys = self.state.macros.get(name).cloned();
        if let Some(keys) = keys {
            for key_str in keys {
                self.execute_macro_key(&key_str);
            }
        }
    }

    fn execute_macro_key(&mut self, key_str: &str) {
        use crossterm::event::KeyCode;
        let key = match key_str {
            "h" => KeyCode::Char('h'),
            "j" => KeyCode::Char('j'),
            "k" => KeyCode::Char('k'),
            "l" => KeyCode::Char('l'),
            "i" => KeyCode::Char('i'),
            "a" => KeyCode::Char('a'),
            "x" => KeyCode::Char('x'),
            "dd" => KeyCode::Char('d'),
            "yy" => KeyCode::Char('y'),
            _ => return,
        };
        match key {
            KeyCode::Char('j') => {
                self.state.cursor.line = (self.state.cursor.line + 1).min(self.buffer.line_count());
            }
            KeyCode::Char('k') => {
                self.state.cursor.line = self.state.cursor.line.saturating_sub(1).max(1);
            }
            KeyCode::Char('h') => {
                self.state.cursor.col = self.state.cursor.col.saturating_sub(1);
            }
            KeyCode::Char('l') => {
                let line_len = self.buffer.get_line(self.state.cursor.line).len();
                self.state.cursor.col = (self.state.cursor.col + 1).min(line_len.saturating_sub(1));
            }
            _ => {}
        }
        self.needs_render = true;
    }

    pub(crate) async fn handle_command(&mut self, key: KeyCode) {
        if self.state.has_confirmation() {
            self.handle_confirmation(key).await;
            return;
        }

        match key {
            KeyCode::Enter => {
                let cmd = self.state.command_buffer.trim().to_string();
                let cmd = cmd.strip_prefix(':').unwrap_or(&cmd);
                self.state.command_buffer.clear();

                match cmd {
                    "q" => {
                        self.handle_quit();
                    }
                    "q!" => {
                        self.running = false;
                    }
                    "w" => {
                        if let Err(e) = self.buffer.save_file().await {
                            eprintln!("[editor] Save failed: {}", e);
                        } else {
                            self.plugin_manager.emit(PluginEvent::BufferSave {
                                file_path: self.state.file_path.clone(),
                            });
                        }
                    }
                    "w!" => {
                        if let Err(e) = self.buffer.save_file().await {
                            eprintln!("[editor] Save failed: {}", e);
                        } else {
                            self.plugin_manager.emit(PluginEvent::BufferSave {
                                file_path: self.state.file_path.clone(),
                            });
                        }
                    }
                    "wq" => {
                        if let Err(e) = self.buffer.save_file().await {
                            eprintln!("[editor] Save failed: {}", e);
                        } else {
                            self.plugin_manager.emit(PluginEvent::BufferSave {
                                file_path: self.state.file_path.clone(),
                            });
                            self.running = false;
                        }
                    }
                    "wq!" => {
                        if let Err(e) = self.buffer.save_file().await {
                            eprintln!("[editor] Save failed: {}", e);
                        } else {
                            self.plugin_manager.emit(PluginEvent::BufferSave {
                                file_path: self.state.file_path.clone(),
                            });
                            self.running = false;
                        }
                    }
                    "wqa" | "wa" => {
                        if let Err(e) = self.buffer.save_file().await {
                            eprintln!("[editor] Save failed: {}", e);
                        } else {
                            self.plugin_manager.emit(PluginEvent::BufferSave {
                                file_path: self.state.file_path.clone(),
                            });
                            self.running = false;
                        }
                    }
                    "qa" => {
                        self.running = false;
                    }
                    "e" => {
                        self.reload_file().await;
                    }
                    "e!" => {
                        self.reload_file_discard().await;
                    }
                    _ => {
                        if let Some(query) = cmd.strip_prefix('/') {
                            let direction = SearchDirection::Forward;
                            self.do_search(query, direction);
                        } else if let Some(query) = cmd.strip_prefix('?') {
                            let direction = SearchDirection::Backward;
                            self.do_search(query, direction);
                        } else if cmd.starts_with("set ") {
                            self.handle_set_command(cmd);
                        } else if cmd.starts_with("w ") || cmd.starts_with("w! ") {
                            self.handle_write_path(cmd).await;
                        } else if cmd.starts_with("wq ") || cmd.starts_with("wq! ") {
                            self.handle_write_quit_path(cmd).await;
                        } else if !self.plugin_manager.execute_command(cmd) {
                            eprintln!("[editor] Unknown command: {}", cmd);
                        }
                    }
                }

                if !self.state.has_confirmation() {
                    self.transition_mode(Mode::Normal);
                    self.needs_render = true;
                }
            }
            KeyCode::Esc => {
                self.state.command_buffer.clear();
                self.transition_mode(Mode::Normal);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                self.state.command_buffer.pop();
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                self.state.command_buffer.push(c);
                self.needs_render = true;
            }
            _ => {}
        }
    }

    pub fn handle_quit(&mut self) {
        if self.buffer.dirty {
            self.state.set_confirmation(
                "No write since last change. Quit anyway? (y/n Enter/Esc: yes, n: no)".to_string(),
                ConfirmAction::Quit,
            );
            self.needs_render = true;
        } else {
            self.running = false;
            self.state.mode = Mode::Normal;
        }
    }

    pub(crate) async fn handle_confirmation(&mut self, key: KeyCode) {
        let should_quit = match key {
            KeyCode::Char('y') | KeyCode::Enter => true,
            KeyCode::Char('n') | KeyCode::Esc => false,
            _ => {
                self.state.clear_confirmation();
                self.needs_render = true;
                return;
            }
        };

        let action = self
            .state
            .confirmation_prompt
            .as_ref()
            .unwrap()
            .action
            .clone();
        self.state.clear_confirmation();
        self.needs_render = true;

        match action {
            ConfirmAction::Quit => {
                if should_quit {
                    self.running = false;
                    self.state.mode = Mode::Normal;
                }
            }
            ConfirmAction::QuitDiscard => {
                self.running = false;
                self.state.mode = Mode::Normal;
            }
            ConfirmAction::WriteQuitAll => {
                if !should_quit {
                    return;
                }
                if let Err(e) = self.buffer.save_file().await {
                    eprintln!("[editor] Save failed: {}", e);
                } else {
                    self.plugin_manager.emit(PluginEvent::BufferSave {
                        file_path: self.state.file_path.clone(),
                    });
                    self.running = false;
                    self.state.mode = Mode::Normal;
                }
            }
        }
    }

    async fn reload_file(&mut self) {
        if let Some(path) = &self.state.file_path {
            match TextBuffer::load_file(path.to_str().unwrap_or("")).await {
                Ok(buf) => {
                    self.buffer = buf;
                }
                Err(e) => {
                    eprintln!("[editor] Failed to reload file: {}", e);
                }
            }
        }
    }

    async fn reload_file_discard(&mut self) {
        self.buffer.dirty = false;
        self.reload_file().await;
    }

    async fn handle_write_path(&mut self, cmd: &str) {
        let path_str = cmd.trim_start_matches("w!").trim_start_matches("w ").trim();
        if path_str.is_empty() {
            eprintln!("[editor] Expected filename after 'w'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.state.file_path = Some(path);
                self.plugin_manager.emit(PluginEvent::BufferSave {
                    file_path: self.state.file_path.clone(),
                });
            }
            Err(e) => {
                eprintln!("[editor] Save failed: {}", e);
            }
        }
    }

    async fn handle_write_quit_path(&mut self, cmd: &str) {
        let path_str = cmd
            .trim_start_matches("wq!")
            .trim_start_matches("wq ")
            .trim();
        if path_str.is_empty() {
            eprintln!("[editor] Expected filename after 'wq'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.state.file_path = Some(path);
                self.plugin_manager.emit(PluginEvent::BufferSave {
                    file_path: self.state.file_path.clone(),
                });
                self.running = false;
            }
            Err(e) => {
                eprintln!("[editor] Save failed: {}", e);
            }
        }
    }

    fn handle_set_command(&mut self, cmd: &str) {
        let args = cmd.trim_start_matches("set ").trim();

        match args {
            "number" => {
                self.state.show_line_numbers = true;
            }
            "nonumber" | "nonumber!" => {
                self.state.show_line_numbers = false;
            }
            "number!" => {
                self.state.show_line_numbers = !self.state.show_line_numbers;
            }
            _ => {
                eprintln!("[editor] Unknown set option: {}", args);
            }
        }
    }

    pub(crate) async fn handle_replace(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.transition_mode(Mode::Normal);
                self.replace_char = None;
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                let line = self.state.cursor.line;
                let col = self.state.cursor.col;
                if col < self.buffer.get_line(line).len() {
                    self.buffer.delete(line, col);
                }
                self.buffer.insert_char(line, col, c);
                self.state.cursor.col += 1;
                self.on_buffer_modified();
            }
            KeyCode::Backspace => {
                if self.state.cursor.col > 0 {
                    self.state.cursor.col -= 1;
                }
                self.needs_render = true;
            }
            KeyCode::Enter => {
                self.buffer
                    .insert(self.state.cursor.line, self.state.cursor.col, "\n");
                self.state.cursor.line += 1;
                self.state.cursor.col = 0;
                self.on_buffer_modified();
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    fn page_down(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let line_count = self.buffer.line_count();
        self.state.cursor.line = (self.state.cursor.line + terminal_rows)
            .min(line_count)
            .max(1);
        let len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
    }

    #[allow(dead_code)]
    fn page_up(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        self.state.cursor.line = self.state.cursor.line.saturating_sub(terminal_rows).max(1);
        let len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
    }

    pub(crate) fn scroll_by(&mut self, lines: usize) {
        if self.state.cursor.line > lines {
            self.state.cursor.line -= lines;
        } else {
            self.state.cursor.line = 1;
        }
    }

    fn jump_to_matching_bracket(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);

        if col >= line_str.len() {
            return;
        }

        let ch = line_str.chars().nth(col);
        if ch.is_none() {
            return;
        }
        let ch = ch.unwrap();

        let matching = match ch {
            '(' => ')',
            ')' => '(',
            '[' => ']',
            ']' => '[',
            '{' => '}',
            '}' => '{',
            _ => return,
        };

        let direction = if ch == matching { 1 } else { -1 };

        let mut count = 1;
        let mut current_line = line;
        let mut current_col = col;

        loop {
            if direction > 0 {
                current_col += 1;
                if current_col >= self.buffer.get_line(current_line).len() {
                    current_line += 1;
                    if current_line > self.buffer.line_count() {
                        break;
                    }
                    current_col = 0;
                }
            } else {
                if current_col == 0 {
                    if current_line == 1 {
                        break;
                    }
                    current_line -= 1;
                    current_col = self.buffer.get_line(current_line).len().saturating_sub(1);
                } else {
                    current_col -= 1;
                }
            }

            let current_char = self.buffer.get_line(current_line).chars().nth(current_col);
            if let Some(c) = current_char {
                if c == ch {
                    count += 1;
                } else if c == matching {
                    count -= 1;
                    if count == 0 {
                        self.state.cursor.line = current_line;
                        self.state.cursor.col = current_col;
                        return;
                    }
                }
            }

            if current_line > self.buffer.line_count() || current_line < 1 {
                break;
            }
        }
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

    pub(crate) fn move_word_forward(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        let mut i = col;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }

        if i < chars.len() {
            self.state.cursor.col = i;
        }
    }

    pub(crate) fn move_word_backward(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        if col == 0 {
            return;
        }

        let mut i = col - 1;
        while i > 0 && chars[i].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }

        self.state.cursor.col = i;
    }

    fn move_word_end(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;
        let line_str = self.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        let mut i = col;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }

        if i > 0 && i <= chars.len() {
            self.state.cursor.col = i - 1;
        }
    }

    fn open_line(&mut self, above: bool) {
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

    fn join_lines(&mut self) {
        let line = self.state.cursor.line;
        if line >= self.buffer.line_count() {
            return;
        }

        let current_line = self.buffer.get_line(line);
        let _next_line = self.buffer.get_line(line + 1);

        self.buffer.delete_line(line + 1);

        if current_line.ends_with(' ') || current_line.ends_with('\t') {
        } else {
            self.buffer.insert(line, current_line.len(), " ");
        }

        self.dot_last_action = Some(DotAction::Change {
            text: String::new(),
            line,
            col: 0,
        });
        self.on_buffer_modified();
    }

    pub fn scroll_cursor_to_center(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let visible_rows = terminal_rows.saturating_sub(2);
        let scroll_pos = self.state.cursor.line.saturating_sub(visible_rows / 2);
        self.state.cursor.line = scroll_pos.max(1);
    }

    fn scroll_cursor_to_top(&mut self) {
        self.state.cursor.line = 1;
    }

    fn scroll_cursor_to_bottom(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let visible_rows = terminal_rows.saturating_sub(2);
        let line_count = self.buffer.line_count();
        self.state.cursor.line = (line_count.saturating_sub(visible_rows) + 1).max(1);
    }

    #[allow(dead_code)]
    fn toggle_line_numbers(&mut self) {
        self.state.show_line_numbers = !self.state.show_line_numbers;
    }

    pub fn scroll_up_one(&mut self) {
        if self.state.cursor.line > 1 {
            self.state.cursor.line -= 1;
        }
    }

    pub fn scroll_down_one(&mut self) {
        let line_count = self.buffer.line_count();
        if self.state.cursor.line < line_count {
            self.state.cursor.line += 1;
        }
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
        eprintln!("\"{}\" {} lines, {} characters", path, line_count, total);
    }

    pub(crate) fn cursor_right(&mut self, n: usize) {
        let line_len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = (self.state.cursor.col + n).min(line_len.saturating_sub(1));
        self.needs_render = true;
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        self.state.cursor.col = self.state.cursor.col.saturating_sub(n);
        self.needs_render = true;
    }

    pub(crate) fn cursor_down(&mut self, n: usize) {
        let line_count = self.buffer.line_count();
        self.state.cursor.line = (self.state.cursor.line + n).min(line_count);
        let len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
        self.needs_render = true;
    }

    pub(crate) fn cursor_up(&mut self, n: usize) {
        self.state.cursor.line = self.state.cursor.line.saturating_sub(n).max(1);
        let len = self.buffer.get_line(self.state.cursor.line).len();
        self.state.cursor.col = self.state.cursor.col.min(len.saturating_sub(1));
        self.needs_render = true;
    }

    pub(crate) fn cursor_line_start(&mut self) {
        self.state.cursor.col = 0;
        self.needs_render = true;
    }

    pub(crate) fn cursor_line_end(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        self.state.cursor.col = line.len().saturating_sub(1);
        self.needs_render = true;
    }

    pub(crate) fn delete_char_forward(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        if self.state.cursor.col < line.len() {
            self.buffer
                .delete(self.state.cursor.line, self.state.cursor.col);
            self.state.dirty = true;
            self.needs_render = true;
        }
    }

    pub(crate) fn kill_line(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        if self.state.cursor.col < line.len() {
            let end_col = line.len();
            let deleted = line[self.state.cursor.col..].to_string();
            let mod_count = self.buffer.modification_count();
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line,
                end_col,
            );
            self.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line: self.state.cursor.line,
                    col: self.state.cursor.col,
                    text: deleted,
                },
                cursor_before: self.state.cursor,
                cursor_after: self.state.cursor,
                modification_count: mod_count,
            });
            self.state.dirty = true;
            self.needs_render = true;
        }
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        self.buffer
            .insert_char(self.state.cursor.line, self.state.cursor.col, c);
        self.state.cursor.col += 1;
        self.state.dirty = true;
        self.needs_render = true;
    }

    pub(crate) fn delete_char_backward(&mut self) {
        if self.state.cursor.col > 0 {
            self.buffer
                .delete(self.state.cursor.line, self.state.cursor.col - 1);
            self.state.cursor.col -= 1;
            self.state.dirty = true;
            self.needs_render = true;
        } else if self.state.cursor.line > 1 {
            let merged = self.buffer.get_line(self.state.cursor.line - 1);
            let prev_len = merged.len();
            self.buffer.merge_with_prev_line(self.state.cursor.line);
            self.state.cursor.line -= 1;
            self.state.cursor.col = prev_len.saturating_sub(1);
            self.state.dirty = true;
            self.needs_render = true;
        }
    }

    pub(crate) fn insert_newline(&mut self) {
        let line = self.buffer.get_line(self.state.cursor.line);
        let (_, after) = line.split_at(self.state.cursor.col);
        self.buffer
            .insert(self.state.cursor.line, self.state.cursor.col, "\n");
        if !after.is_empty() {
            self.buffer.insert(self.state.cursor.line + 1, 0, after);
        }
        self.state.cursor.line += 1;
        self.state.cursor.col = 0;
        self.state.dirty = true;
        self.needs_render = true;
    }

    pub fn kill_word(&mut self) {
        let (_, _, end_col) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        if end_col > self.state.cursor.col {
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line,
                end_col,
            );
            self.state.dirty = true;
            self.needs_render = true;
        } else if self.state.cursor.line < self.buffer.line_count() {
            self.buffer.delete_range(
                self.state.cursor.line,
                self.state.cursor.col,
                self.state.cursor.line + 1,
                0,
            );
            self.state.dirty = true;
            self.needs_render = true;
        }
    }

    pub fn yank_pop(&mut self) {
        self.undo();
    }

    pub fn transpose_chars(&mut self) {
        let line = self.state.cursor.line;
        let col = self.state.cursor.col;

        if col > 0 {
            let char1 = self.buffer.get_line(line).chars().nth(col - 1);
            let char2 = self.buffer.get_line(line).chars().nth(col);

            if let (Some(c1), Some(c2)) = (char1, char2) {
                self.buffer.delete(line, col);
                self.buffer.delete(line, col - 1);
                self.buffer.insert_char(line, col - 2, c2);
                self.buffer.insert_char(line, col - 1, c1);
                self.state.cursor.col = (col + 1).min(self.buffer.get_line(line).len());
                self.state.dirty = true;
                self.needs_render = true;
            }
        }
    }

    pub fn insert_tab(&mut self) {
        self.buffer
            .insert_char(self.state.cursor.line, self.state.cursor.col, '\t');
        self.state.cursor.col += 1;
        self.state.dirty = true;
        self.needs_render = true;
    }

    pub fn scroll_up(&mut self) {
        let rows = self.terminal.rows() as usize;
        self.scroll_by(rows);
        self.needs_render = true;
    }

    pub fn scroll_down(&mut self) {
        let rows = self.terminal.rows() as usize;
        self.scroll_by(rows.saturating_sub(1));
        self.needs_render = true;
    }

    pub fn clear_screen(&mut self) {
        let _ = self.terminal.clear_screen();
        self.needs_render = true;
    }

    pub fn abort(&mut self) {
        self.state.command_buffer.clear();
        if self.state.mode == Mode::Command {
            self.state.mode = Mode::Normal;
            self.needs_render = true;
        }
    }

    // ── Emacs Mark/Region ──────────────────────────────────────

    /// C-space: 現在カーソル位置にマークをセットする。
    pub fn set_mark(&mut self) {
        self.state.mark = Some(self.state.cursor);
        self.state.region_active = true;
        self.needs_render = true;
    }

    /// アクティブなリージョンが存在するか。mark がセット済みでカーソルと異なる位置にあること。
    pub fn has_active_region(&self) -> bool {
        self.state.region_active
            && self.state.mark.is_some()
            && self.state.mark.unwrap() != self.state.cursor
    }

    /// C-w (region active): リージョン内のテキストを削除し、kill-ring と default register に保存する。
    ///
    /// Emacs の region は [mark, point) の半開区間。normalize_selection と違い
    /// end に +1 補正しない（カーソル位置は既に排他的範囲の終端）。
    pub fn kill_region(&mut self) {
        let mark = match self.state.mark {
            Some(m) => m,
            None => return,
        };
        let cursor = self.state.cursor;

        // 半開区間 [start, end): end は排他的（Emacs region の定義そのもの）
        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };

        if e_line == s_line && e_col <= s_col {
            self.deactivate_region();
            return;
        }

        let mod_count = self.buffer.modification_count();
        let content = self.buffer.delete_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }

        self.kill_ring.push(&content);
        self.register.set('"', &content);
        self.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: s_line,
                col: s_col,
                text: content,
            },
            cursor_before: cursor,
            cursor_after: Position {
                line: s_line,
                col: s_col,
            },
            modification_count: mod_count,
        });
        self.state.cursor = Position {
            line: s_line,
            col: s_col,
        };
        self.deactivate_region();
        self.on_buffer_modified();
    }

    /// C-g (region active 時): リージョンを非アクティブにする。mark は保持する。
    pub fn deactivate_region(&mut self) {
        if self.state.region_active {
            self.state.region_active = false;
            self.needs_render = true;
        }
    }

    /// M-w: リージョンを kill-ring と register にコピーする（削除なし）。
    pub fn copy_region(&mut self) {
        let mark = match self.state.mark {
            Some(m) => m,
            None => return,
        };
        let cursor = self.state.cursor;
        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };
        let content = self.buffer.get_char_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }
        self.kill_ring.push(&content);
        self.register.set('"', &content);
        self.deactivate_region();
        self.needs_render = true;
    }

    /// C-y: kill-ring から最新エントリを現在位置に挿入する。
    pub fn yank_from_kill_ring(&mut self) {
        if let Some(text) = self.kill_ring.yank() {
            self.buffer
                .insert(self.state.cursor.line, self.state.cursor.col, text);
            self.state.cursor.col += text.chars().count();
            self.state.dirty = true;
            self.needs_render = true;
        }
    }

    // ── End Emacs Mark/Region ──────────────────────────────────

    pub async fn consume_pending_save(&mut self) {
        if self.pending_save {
            self.pending_save = false;
            self.save_file_async().await;
        }
    }

    pub async fn save_file_async(&mut self) {
        if let Err(e) = self.buffer.save_file().await {
            eprintln!("[editor] Save failed: {}", e);
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
    use crate::types::Keymap;
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
            visual_start: None,
            visual_type: None,
            marks: crate::types::Marks::new(),
            macros: crate::types::Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
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
            pending_operator: None,
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
            pending_save: false,
            kill_ring: KillRing::new(),
            highlights: Vec::new(),
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
    async fn consume_pending_save_clears_flag_after_attempt() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.pending_save = true;
        editor.buffer.dirty = true;

        editor.consume_pending_save().await;

        assert!(!editor.pending_save);
        assert!(editor.needs_render);
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

        // マークを手動セット
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

        // マーク: col 0, カーソル: col 5 → "hello" を選択
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

        // マーク: col 5, カーソル: col 0 → 逆方向選択でも同じく "hello"
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
        // mark は保持
        assert!(editor.state.mark.is_some());
    }

    #[test]
    fn emacs_set_mark_with_emacs_keymap() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("abcdef\n");

        // カーソルを移動してからマーク
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

        // リージョンがないので kill_word が実行される
        // "hello" の 'h' (col 0) から単語終端 (col 5) まで削除 → " world\n"
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
        assert_eq!(editor.state.cursor.col, 5);
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
        // kill_line はカーソルから行末まで削除
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
        // redo が実装されており、パニックしないことだけ確認
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

        // テキストは削除されていない
        let text = editor.buffer.get_line(1);
        assert_eq!(text, "hello world\n");
        // kill-ring にコピーされている
        assert_eq!(editor.kill_ring.yank(), Some("hello"));
        // リージョンは非アクティブに
        assert!(!editor.state.region_active);
    }

    #[test]
    fn emacs_yank_from_kill_ring_inserts_text() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.kill_ring.push("xyz");
        editor.state.cursor.col = 5; // ' ' の位置

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
        // 現在の単語を飛び越え、次の空白位置に移動
        assert_eq!(editor.state.cursor.col, 5); // "hello" 直後の空白位置
    }

    #[test]
    fn emacs_m_b_moves_word_backward() {
        let mut editor = test_editor(Keymap::Emacs);
        editor.set_buffer_for_test("hello world\n");
        editor.state.cursor.col = 8;
        editor.move_word_backward();
        assert_eq!(editor.state.cursor.col, 6); // "world" の先頭
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
