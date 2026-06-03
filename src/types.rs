use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IjevimError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plugin error: {0}")]
    Plugin(String),
    #[error("Terminal error: {0}")]
    Terminal(String),
    #[error("No file path set")]
    NoFilePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Keymap {
    #[default]
    Vim,
    Emacs,
}

impl std::fmt::Display for Keymap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Keymap::Vim => write!(f, "vim"),
            Keymap::Emacs => write!(f, "emacs"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Visual,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualType {
    Character,
    Line,
    Block,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub line: usize,
    pub start_col: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, Default)]
pub struct Marks {
    marks: HashMap<char, Position>,
}

impl Marks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: char, position: Position) {
        self.marks.insert(name, position);
    }

    pub fn get(&self, name: char) -> Option<Position> {
        self.marks.get(&name).copied()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Macros {
    macros: HashMap<char, Vec<String>>,
    recording: Option<char>,
}

impl Macros {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_recording(&mut self, name: char) {
        self.recording = Some(name);
        self.macros.entry(name).or_default();
    }

    pub fn stop_recording(&mut self) -> Option<char> {
        self.recording.take()
    }

    pub fn add_key(&mut self, key: String) {
        if let Some(name) = self.recording {
            if let Some(keys) = self.macros.get_mut(&name) {
                keys.push(key);
            }
        }
    }

    pub fn get(&self, name: char) -> Option<&Vec<String>> {
        self.macros.get(&name)
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Normal => write!(f, "NORMAL"),
            Mode::Insert => write!(f, "INSERT"),
            Mode::Command => write!(f, "COMMAND"),
            Mode::Visual => write!(f, "VISUAL"),
            Mode::Replace => write!(f, "REPLACE"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    pub mode: Mode,
    pub cursor: Position,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    pub command_buffer: String,
    pub visual_start: Option<Position>,
    pub visual_type: Option<VisualType>,
    pub marks: Marks,
    pub macros: Macros,
    pub confirmation_prompt: Option<ConfirmationPrompt>,
    pub show_line_numbers: bool,
    pub wrap: bool,
    pub mark: Option<Position>,
    pub region_active: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            cursor: Position { line: 1, col: 0 },
            file_path: None,
            dirty: false,
            command_buffer: String::new(),
            visual_start: None,
            visual_type: None,
            marks: Marks::new(),
            macros: Macros::new(),
            confirmation_prompt: None,
            show_line_numbers: true,
            wrap: true,
            mark: None,
            region_active: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PluginEvent {
    ModeChange {
        from: Mode,
        to: Mode,
    },
    BufferChange,
    Key {
        mode: Mode,
        /// String representation of the key (e.g. "a", "Enter", "Esc", "F1").
        /// For `KeyCode::Char(c)`, this is `c.to_string()`.
        /// For other keys, this is the Debug name.
        key: String,
    },
    BufferSave {
        file_path: Option<PathBuf>,
    },
    /// All plugins have been loaded and their `setup()` methods called.
    /// The editor event loop is about to start (or has just started).
    Ready,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Quit,
    QuitDiscard,
    WriteQuitAll,
}

#[derive(Clone, Debug)]
pub struct ConfirmationPrompt {
    pub message: String,
    pub action: ConfirmAction,
}

impl EditorState {
    pub fn has_confirmation(&self) -> bool {
        self.confirmation_prompt.is_some()
    }

    pub fn set_confirmation(&mut self, message: String, action: ConfirmAction) {
        self.confirmation_prompt = Some(ConfirmationPrompt { message, action });
    }

    pub fn clear_confirmation(&mut self) {
        self.confirmation_prompt = None;
    }
}
