use crate::types::IjevimError;
use ropey::Rope;
use std::path::PathBuf;

pub struct TextBuffer {
    doc: Rope,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    modification_count: usize,
    pub(crate) saved_modification_count: usize,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            doc: Rope::new(),
            file_path: None,
            dirty: false,
            modification_count: 0,
            saved_modification_count: 0,
        }
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            doc: Rope::from_str(text),
            file_path: None,
            dirty: false,
            modification_count: 0,
            saved_modification_count: 0,
        }
    }

    pub fn line_count(&self) -> usize {
        self.doc.len_lines()
    }

    pub fn get_line(&self, line_number: usize) -> String {
        if line_number < 1 || line_number > self.line_count() {
            return String::new();
        }
        self.doc.line(line_number - 1).to_string()
    }

    pub fn insert(&mut self, line: usize, col: usize, text: &str) {
        let line_idx = line.saturating_sub(1);
        if line_idx >= self.doc.len_lines() {
            if self.doc.len_chars() == 0 {
                self.doc.insert(0, "\n");
            } else if line_idx == self.doc.len_lines() {
                self.doc.insert(self.doc.len_chars(), "\n");
            } else {
                return;
            }
            self.dirty = true;
            self.modification_count += 1;
            if text != "\n" {
                let insert_pos = self.doc.len_chars();
                self.doc.insert(insert_pos, text);
                self.modification_count += 1;
            }
            return;
        }

        let line_start = self.doc.line_to_char(line_idx);
        let char_idx = line_start + col.min(self.doc.line(line_idx).len_chars());

        self.doc.insert(char_idx, text);
        self.dirty = true;
        self.modification_count += 1;
    }

    pub fn delete(&mut self, line: usize, col: usize) {
        let line_idx = line.saturating_sub(1);
        if line_idx >= self.doc.len_lines() {
            return;
        }

        let line_start = self.doc.line_to_char(line_idx);
        let char_idx = line_start + col;

        if char_idx >= self.doc.len_chars() {
            return;
        }

        self.doc.remove(char_idx..char_idx + 1);
        self.dirty = true;
        self.modification_count += 1;
    }

    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        self.insert(line, col, &ch.to_string());
    }

    pub fn insert_newline(&mut self, line: usize, col: usize) {
        self.insert(line, col, "\n");
    }

    pub fn merge_with_prev_line(&mut self, line: usize) -> usize {
        if line <= 1 || line > self.line_count() {
            return 0;
        }

        let prev_line_idx = line - 2;
        let cur_line_idx = line - 1;

        if prev_line_idx >= self.doc.len_lines() || cur_line_idx >= self.doc.len_lines() {
            return 0;
        }

        let prev_line_start = self.doc.line_to_char(prev_line_idx);
        let prev_line_len = self.doc.line(prev_line_idx).len_chars();
        let newline_pos = prev_line_start + prev_line_len - 1;

        if newline_pos >= self.doc.len_chars() {
            return 0;
        }

        self.doc.remove(newline_pos..newline_pos + 1);
        self.dirty = true;
        self.modification_count += 1;

        prev_line_len - 1
    }

    pub async fn load_file(path: &str) -> Result<Self, IjevimError> {
        let content = tokio::fs::read_to_string(path).await?;
        let mut buffer = Self::with_text(&content);
        buffer.file_path = Some(PathBuf::from(path));
        Ok(buffer)
    }

    pub async fn save_file(&mut self) -> Result<(), IjevimError> {
        if let Some(path) = self.file_path.clone() {
            self.save_to_path(&path).await
        } else {
            Err(IjevimError::NoFilePath)
        }
    }

    pub async fn save_to_path(&mut self, path: &std::path::Path) -> Result<(), IjevimError> {
        let content = self.doc.to_string();
        tokio::fs::write(path, content).await?;
        self.file_path = Some(path.to_path_buf());
        self.saved_modification_count = self.modification_count;
        self.dirty = false;
        Ok(())
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.doc.to_string()
    }

    pub fn modification_count(&self) -> usize {
        self.modification_count
    }

    pub fn is_clean(&self) -> bool {
        self.modification_count == self.saved_modification_count
    }

    pub fn reset_clean_state(&mut self) {
        self.saved_modification_count = self.modification_count;
        self.dirty = false;
    }

    pub(crate) fn reset_modification_count(&mut self, count: usize) {
        self.modification_count = count;
    }

    pub(crate) fn replace_with(&mut self, text: &str) {
        let len = self.doc.len_chars();
        if len > 0 {
            self.doc.remove(0..len);
        }
        self.doc.insert(0, text);
        self.dirty = true;
        self.modification_count += 1;
    }

    pub fn line_to_char(&self, line_idx: usize) -> usize {
        self.doc.line_to_char(line_idx)
    }

    pub fn len_chars(&self) -> usize {
        self.doc.len_chars()
    }

    pub fn get_word_at(&self, line: usize, col: usize) -> (String, usize, usize) {
        let line_idx = line.saturating_sub(1);
        if line_idx >= self.doc.len_lines() {
            return (String::new(), 0, 0);
        }
        let line_str = self.doc.line(line_idx).to_string();
        let chars: Vec<char> = line_str.chars().collect();

        if col >= chars.len() {
            return (String::new(), col, col);
        }

        let mut start = col;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let word: String = chars[start..end].iter().collect();
        (word, start, end)
    }

    pub fn get_word_range(&self, line: usize, col: usize) -> (String, usize, usize) {
        self.get_word_at(line, col)
    }

    pub fn get_line_range(&self, start_line: usize, end_line: usize) -> String {
        if start_line > end_line || start_line < 1 {
            return String::new();
        }
        let start = start_line.saturating_sub(1);
        let end = end_line.min(self.doc.len_lines());
        if start >= end {
            return String::new();
        }
        let mut result = String::new();
        for i in start..end {
            result.push_str(&self.doc.line(i).to_string());
            if i < end - 1 {
                result.push('\n');
            }
        }
        result
    }

    pub fn get_char_range(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line {
            return String::new();
        }
        if start_line == end_line {
            if start_col >= end_col {
                return String::new();
            }
            let line_str = self.get_line(start_line);
            return line_str
                .chars()
                .skip(start_col)
                .take(end_col - start_col)
                .collect();
        }

        let mut result = String::new();
        result.push_str(
            &self
                .get_line(start_line)
                .chars()
                .skip(start_col)
                .collect::<String>(),
        );
        result.push('\n');
        for line_num in (start_line + 1)..end_line {
            result.push_str(&self.get_line(line_num));
            result.push('\n');
        }
        result.push_str(
            &self
                .get_line(end_line)
                .chars()
                .take(end_col)
                .collect::<String>(),
        );
        result
    }

    pub fn delete_line(&mut self, line: usize) -> String {
        if line < 1 || line > self.line_count() {
            return String::new();
        }
        let line_idx = line.saturating_sub(1);
        let content = self.doc.line(line_idx).to_string();

        let char_start = self.doc.line_to_char(line_idx);
        let char_end = if line_idx + 1 < self.doc.len_lines() {
            self.doc.line_to_char(line_idx + 1)
        } else {
            self.doc.len_chars()
        };

        if char_end > char_start {
            self.doc.remove(char_start..char_end);
            // Ensure buffer always has at least one newline
            if self.doc.len_chars() == 0 {
                self.doc.insert(0, "\n");
            }
            self.dirty = true;
            self.modification_count += 1;
        }

        content
    }

    pub fn delete_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line {
            return String::new();
        }

        let content = self.get_char_range(start_line, start_col, end_line, end_col);

        let start_line_idx = start_line.saturating_sub(1);
        let end_line_idx = end_line.saturating_sub(1);

        let char_start = self.doc.line_to_char(start_line_idx)
            + start_col.min(self.doc.line(start_line_idx).len_chars());
        let char_end = self.doc.line_to_char(end_line_idx)
            + end_col.min(self.doc.line(end_line_idx).len_chars());

        if char_start < char_end {
            self.doc.remove(char_start..char_end);
            self.dirty = true;
            self.modification_count += 1;
        }

        content
    }

    pub fn remove_range(&mut self, start: usize, end: usize) {
        if start < end {
            self.doc.remove(start..end);
            self.dirty = true;
            self.modification_count += 1;
        }
    }

    pub fn search(&self, query: &str) -> Vec<crate::types::SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_chars: Vec<char> = query.chars().collect();
        let mut results = Vec::new();

        for line_idx in 0..self.doc.len_lines() {
            let line = self.doc.line(line_idx);
            let line_str = line.to_string();
            let line_chars: Vec<char> = line_str.chars().collect();

            let mut col = 0;
            while col <= line_chars.len().saturating_sub(query_chars.len()) {
                let mut matches = true;
                for (i, &qc) in query_chars.iter().enumerate() {
                    if col + i >= line_chars.len() || line_chars[col + i] != qc {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    results.push(crate::types::SearchResult {
                        line: line_idx + 1,
                        start_col: col,
                    });
                    col += 1;
                } else {
                    col += 1;
                }
            }
        }

        results
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char(1, 0, 'a');
        assert_eq!(buf.get_line(1), "a");
        assert!(buf.dirty);
        assert_eq!(buf.modification_count(), 1);
    }

    #[test]
    fn test_insert_at_end_of_buffer() {
        let mut buf = TextBuffer::new();
        buf.insert(1, 0, "hello");
        assert_eq!(buf.get_line(1), "hello");
        assert!(buf.dirty);
    }

    #[test]
    fn test_delete_char() {
        let mut buf = TextBuffer::with_text("ab\n");
        buf.delete(1, 0);
        assert_eq!(buf.get_line(1), "b\n");
        assert!(buf.dirty);
    }

    #[test]
    fn test_merge_with_prev_line() {
        let mut buf = TextBuffer::with_text("hello\nworld\n");
        buf.merge_with_prev_line(2);
        assert_eq!(buf.to_string(), "helloworld\n");
        assert!(buf.dirty);
    }

    #[test]
    fn test_save_resets_dirty() {
        let mut buf = TextBuffer::with_text("test\n");
        buf.file_path = Some(std::path::PathBuf::from("/tmp/test_ijevim.txt"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            buf.save_file().await.expect("Save failed");
        });
        assert!(!buf.dirty);
    }
}
