use crate::buffer::TextBuffer;
use crate::terminal::Terminal;
use crate::types::{EditorState, Mode};
use std::io;

pub struct Renderer {
    scroll_top: usize,
    last_modification_count: usize,
    last_line_count: usize,
    last_status: String,
    last_confirmation: Option<String>,
    show_line_numbers: bool,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            scroll_top: 1,
            last_modification_count: 0,
            last_line_count: 0,
            last_status: String::new(),
            last_confirmation: None,
            show_line_numbers: true,
        }
    }

    pub fn render(
        &mut self,
        terminal: &mut Terminal,
        buffer: &TextBuffer,
        state: &EditorState,
    ) -> io::Result<()> {
        let rows = terminal.rows().max(2);
        let visible_rows = (rows as usize).saturating_sub(2).max(1);

        let line_number_width = if state.show_line_numbers {
            let total_lines = buffer.line_count();
            if total_lines == 0 {
                1
            } else {
                total_lines.to_string().len()
            }
        } else {
            0
        };

        let mode_label = if state.region_active {
            if state.mark == Some(state.cursor) {
                " MARK "
            } else {
                " REGION "
            }
        } else {
            match state.mode {
                Mode::Normal => " NORMAL ",
                Mode::Insert => " INSERT ",
                Mode::Visual => " VISUAL ",
                Mode::Command => " COMMAND ",
                Mode::Replace => " REPLACE ",
            }
        };

        let status = if state.has_confirmation() {
            state.confirmation_prompt.as_ref().unwrap().message.clone()
        } else if state.mode == Mode::Command {
            format!(":{}", state.command_buffer)
        } else if state.mode == Mode::Replace {
            let line = state.cursor.line;
            let col = state.cursor.col + 1;
            format!(
                "--{}-- {} {}:{}",
                mode_label,
                state
                    .file_path
                    .as_ref()
                    .map_or("[No Name]", |p| p.to_str().unwrap_or("[Invalid Path]")),
                line,
                col
            )
        } else {
            format!(
                "--{}-- {} {}{}",
                mode_label,
                state
                    .file_path
                    .as_ref()
                    .map_or("[No Name]", |p| p.to_str().unwrap_or("[Invalid Path]")),
                if state.dirty { "[+]" } else { "" },
                if state.show_line_numbers { " " } else { "" }
            )
        };

        let confirmation_msg = state
            .confirmation_prompt
            .as_ref()
            .map(|c| c.message.clone());
        let needs_full_render = buffer.modification_count() != self.last_modification_count
            || self.last_line_count != buffer.line_count()
            || status != self.last_status
            || confirmation_msg != self.last_confirmation
            || self.show_line_numbers != state.show_line_numbers;

        if needs_full_render {
            self.scroll_top = self
                .scroll_top
                .clamp(1, buffer.line_count().saturating_sub(visible_rows).max(1));
            self.show_line_numbers = state.show_line_numbers;
        }

        if state.cursor.line < self.scroll_top {
            self.scroll_top = state.cursor.line;
        } else if state.cursor.line >= self.scroll_top + visible_rows {
            self.scroll_top = state.cursor.line - visible_rows + 1;
        }

        self.last_modification_count = buffer.modification_count();
        self.last_line_count = buffer.line_count();

        if needs_full_render {
            terminal.clear_screen()?;

            for i in 0..visible_rows {
                let buf_line = self.scroll_top + i;
                let raw_line = buffer.get_line(buf_line);
                let display_text = if raw_line.is_empty()
                    && buf_line > buffer.line_count()
                    && buffer.line_count() > 0
                {
                    if state.show_line_numbers {
                        format!("{:width$}", "~", width = line_number_width)
                    } else {
                        "~".to_string()
                    }
                } else {
                    if state.show_line_numbers {
                        let line_num = if buf_line <= buffer.line_count() {
                            buf_line
                        } else {
                            0
                        };
                        let num_str = format!("{:width$}  ", line_num, width = line_number_width);
                        let gray = "\x1b[38;5;240m";
                        let reset = "\x1b[0m";
                        format!("{}{}{}{}", gray, num_str, reset, raw_line)
                    } else {
                        raw_line
                    }
                };

                terminal.write_line((i + 1) as u16, &display_text)?;
            }

            terminal.write_status(&status)?;
            self.last_status = status;
            self.last_confirmation = confirmation_msg;
        }

        let screen_row = (state.cursor.line.saturating_sub(self.scroll_top) + 1) as u16;
        let screen_col = if state.show_line_numbers {
            (state.cursor.col + line_number_width + 2) as u16
        } else {
            (state.cursor.col + 1) as u16
        };
        terminal.move_cursor(screen_row, screen_col)?;
        terminal.flush()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
