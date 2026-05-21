use crate::buffer::TextBuffer;
use crate::types::Position;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum EditType {
    Insert {
        line: usize,
        col: usize,
        text: String,
    },
    Delete {
        line: usize,
        col: usize,
        text: String,
    },
    InsertLine {
        line: usize,
        text: String,
    },
    DeleteLine {
        line: usize,
        text: String,
    },
    Replace {
        line: usize,
        col: usize,
        old_text: String,
        new_text: String,
    },
    Merge {
        line: usize,
        deleted_newline_col: usize,
    },
    Split {
        line: usize,
        col: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub edit_type: EditType,
    pub cursor_before: Position,
    #[allow(dead_code)]
    pub cursor_after: Position,
    pub modification_count: usize,
}

pub struct UndoManager {
    undo_stack: VecDeque<Edit>,
    redo_stack: VecDeque<Edit>,
    max_size: usize,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_size: 1000,
        }
    }

    #[allow(dead_code)]
    pub fn push(&mut self, edit: Edit) {
        if self.undo_stack.len() >= self.max_size {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(edit);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, buffer: &mut TextBuffer) -> Option<Edit> {
        let edit = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(edit.clone());
        self.apply_undo(buffer, &edit);
        buffer.reset_modification_count(edit.modification_count);
        Some(edit)
    }

    #[allow(dead_code)]
    pub fn redo(&mut self, buffer: &mut TextBuffer) -> Option<Edit> {
        let edit = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(edit.clone());
        self.apply_redo(buffer, &edit);
        buffer.reset_modification_count(edit.modification_count);
        Some(edit)
    }

    fn apply_undo(&self, buffer: &mut TextBuffer, edit: &Edit) {
        match &edit.edit_type {
            EditType::Insert { line, col, text } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + text.chars().count();
                buffer.remove_range(start, end);
            }
            EditType::Delete { line, col, text } => {
                buffer.insert(*line, *col, text);
            }
            EditType::InsertLine { line, text: _ } => {
                buffer.delete_line(*line);
            }
            EditType::DeleteLine { line, text } => {
                // If the buffer is just a single newline (last line was deleted),
                // replace it with the deleted text.
                if buffer.to_string() == "\n" {
                    buffer.replace_with(text);
                } else {
                    buffer.insert(*line, 0, text);
                }
            }
            EditType::Replace {
                line,
                col,
                old_text,
                new_text,
            } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + new_text.chars().count();
                buffer.remove_range(start, end);
                buffer.insert(*line, *col, old_text);
            }
            EditType::Merge {
                line,
                deleted_newline_col: _,
            } => {
                let line_idx = line.saturating_sub(1);
                if line_idx + 1 < buffer.line_count() {
                    buffer.insert(line_idx, buffer.get_line(line_idx).len(), "\n");
                }
            }
            EditType::Split { line, col } => {
                let start = buffer.line_to_char(line - 1) + col;
                if start < buffer.len_chars() && start > 0 {
                    buffer.remove_range(start, start + 1);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn apply_redo(&self, buffer: &mut TextBuffer, edit: &Edit) {
        match &edit.edit_type {
            EditType::Insert { line, col, text } => {
                buffer.insert(*line, *col, text);
            }
            EditType::Delete { line, col, text } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + text.chars().count();
                buffer.remove_range(start, end);
            }
            EditType::InsertLine { line, text } => {
                buffer.insert(*line, 0, text);
                buffer.insert(*line, text.len(), "\n");
            }
            EditType::DeleteLine { line, text: _ } => {
                buffer.delete_line(*line);
            }
            EditType::Replace {
                line,
                col,
                old_text,
                new_text,
            } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + old_text.chars().count();
                buffer.remove_range(start, end);
                buffer.insert(*line, *col, new_text);
            }
            EditType::Merge {
                line,
                deleted_newline_col: _,
            } => {
                buffer.merge_with_prev_line(*line);
            }
            EditType::Split { line, col } => {
                buffer.insert(*line, *col, "\n");
            }
        }
    }

    #[allow(dead_code)]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    #[allow(dead_code)]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new()
    }
}
