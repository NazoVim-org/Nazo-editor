use crate::buffer::TextBuffer;
use crate::screen::{build_styled_chars, CellStyle, ScreenBuffer};
use crate::terminal::Terminal;
use crate::types::{EditorState, Mode};
use crossterm::style::Color;
use std::io;
use syntect::highlighting::Style;

/// One visual (display) row produced from wrapping a buffer line.
struct VisualLine {
    /// Buffer line number (1-indexed).
    buffer_line: usize,
    /// Starting buffer column of this visual segment.
    segment_start_col: usize,
    /// Styled characters to display on this screen row.
    styled_chars: Vec<(char, CellStyle)>,
}

pub struct Renderer {
    /// Screen cell buffer for flicker-free diff-based rendering.
    screen: ScreenBuffer,
    /// The buffer line (1-indexed) displayed at the top of the content area.
    scroll_top: usize,
    /// Cached show_line_numbers for dirty detection.
    show_line_numbers: bool,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            screen: ScreenBuffer::new(24, 80),
            scroll_top: 1,
            show_line_numbers: true,
        }
    }

    /// Main entry point: compose a frame into the screen buffer and emit
    /// minimal ANSI diffs to the terminal.
    ///
    /// `highlights` is the per‑line syntect highlight data
    /// (`editor.highlights`, indexed by 0‑based buffer line).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        terminal: &mut Terminal,
        buffer: &TextBuffer,
        state: &EditorState,
        highlights: &[Vec<(Style, String)>],
    ) -> io::Result<()> {
        let rows = terminal.rows() as usize;
        let cols = terminal.cols() as usize;
        let visible_rows = rows.saturating_sub(2).max(1);

        // ── Resize screen if needed (handles terminal resize) ──
        let size_changed = self.screen.rows != rows || self.screen.cols != cols;
        self.screen.resize(rows, cols);
        if size_changed {
            self.screen.invalidate_all();
        }
        self.screen.clear_all();

        self.show_line_numbers = state.show_line_numbers;

        // ── Line‑number metrics ──
        let line_number_width = if state.show_line_numbers {
            let total = buffer.line_count().max(1);
            total.to_string().len()
        } else {
            0
        };
        let line_number_prefix = if state.show_line_numbers {
            line_number_width + 2
        } else {
            0
        };
        let content_width = cols.saturating_sub(line_number_prefix).max(1);

        // ── Build visual layout (buffer lines → wrapped screen rows) ──
        let mut visual_lines: Vec<VisualLine> = Vec::with_capacity(visible_rows);

        // Ensure cursor is visible
        self.ensure_cursor_visible(
            buffer,
            content_width,
            state.wrap,
            visible_rows,
            state.cursor.line,
        );

        // Build layout
        let mut buf_line = self.scroll_top;
        let mut screen_rows_used = 0;
        while screen_rows_used < visible_rows && buf_line <= buffer.line_count() {
            let raw_line = buffer.get_line(buf_line);
            let line_highlights = highlights
                .get(buf_line.saturating_sub(1))
                .map(|v| v.as_slice());
            let styled_chars = build_styled_chars(&raw_line, line_highlights);
            let segments = wrap_styled(&styled_chars, content_width, state.wrap);

            for (seg_idx, segment) in segments.iter().enumerate() {
                if screen_rows_used >= visible_rows {
                    break;
                }
                visual_lines.push(VisualLine {
                    buffer_line: buf_line,
                    segment_start_col: seg_idx * content_width,
                    styled_chars: segment.clone(),
                });
                screen_rows_used += 1;
            }
            buf_line += 1;
        }

        // ── Render content rows into screen buffer ──
        let gray_style = CellStyle {
            fg: Color::AnsiValue(240),
            ..CellStyle::default()
        };

        for (screen_row, vline) in visual_lines.iter().enumerate() {
            // Line number prefix
            if state.show_line_numbers {
                let num_str = format!("{:width$}  ", vline.buffer_line, width = line_number_width);
                for (col, ch) in num_str.chars().enumerate() {
                    let style = if col < line_number_width {
                        gray_style
                    } else {
                        CellStyle::default()
                    };
                    self.screen.set_cell(screen_row, col, ch, style);
                }
            }

            // Styled content
            for (col, &(ch, style)) in vline.styled_chars.iter().enumerate() {
                let screen_col = line_number_prefix + col;
                if screen_col < cols {
                    self.screen.set_cell(screen_row, screen_col, ch, style);
                }
            }

            // Clear remainder of line (handles partial wrapped segments, etc.)
            let content_end = line_number_prefix + vline.styled_chars.len();
            if content_end < cols {
                for c in content_end..cols {
                    self.screen
                        .set_cell(screen_row, c, ' ', CellStyle::default());
                }
            }
        }

        // ── Tilde lines (beyond end of buffer) ──
        for screen_row in visual_lines.len()..visible_rows {
            if state.show_line_numbers {
                let num_str = format!("{:width$}  ", "~", width = line_number_width);
                for (col, ch) in num_str.chars().enumerate() {
                    let style = if col < line_number_width {
                        gray_style
                    } else {
                        CellStyle::default()
                    };
                    self.screen.set_cell(screen_row, col, ch, style);
                }
                // Show ~ in first content column
                self.screen
                    .set_cell(screen_row, line_number_prefix, '~', CellStyle::default());
            } else {
                self.screen
                    .set_cell(screen_row, 0, '~', CellStyle::default());
            }
        }

        // ── Status line ──
        let status_row = rows.saturating_sub(1);
        let is_show_message = state.last_message.is_some();
        let status_style = if is_show_message {
            CellStyle {
                bg: Color::Red,
                fg: Color::White,
                bold: true,
                ..CellStyle::default()
            }
        } else {
            CellStyle {
                bg: Color::AnsiValue(236),
                fg: Color::White,
                ..CellStyle::default()
            }
        };
        self.render_status(status_row, cols, state, status_style);

        // ── Cursor screen position ──
        let (cursor_row, cursor_col) = self.cursor_to_screen(
            &visual_lines,
            state.cursor.line,
            state.cursor.col,
            line_number_prefix,
            content_width,
            state.wrap,
            buffer,
        );
        let cursor_row = cursor_row.min(visible_rows.saturating_sub(1));
        let cursor_col = cursor_col.min(cols.saturating_sub(1));

        // ── Emit frame to terminal ──
        let ansi_output = self.screen.render();
        if !ansi_output.is_empty() {
            terminal.write_raw(ansi_output.as_bytes())?;
        }

        terminal.move_cursor(
            ((cursor_row + 1).max(1).min(rows)) as u16,
            ((cursor_col + 1).max(1).min(cols)) as u16,
        )?;
        terminal.flush()?;

        Ok(())
    }

    // ── Scroll helpers ──

    /// Ensure the cursor buffer line is visible in the viewport.
    fn ensure_cursor_visible(
        &mut self,
        buffer: &TextBuffer,
        content_width: usize,
        wrap_enabled: bool,
        visible_rows: usize,
        cursor_line: usize,
    ) {
        // Cursor above scroll area
        if cursor_line < self.scroll_top {
            self.scroll_top = cursor_line;
            return;
        }

        // Count wrapped rows from scroll_top to cursor_line
        let mut rows_consumed = 0;
        let mut line = self.scroll_top;
        while line < cursor_line && line <= buffer.line_count() {
            let text = buffer.get_line(line);
            rows_consumed += wrapped_rows(&text, content_width, wrap_enabled);
            if rows_consumed >= visible_rows {
                // Cursor is below — scroll to make cursor visible
                let new_top = self.scroll_up_to_fit(
                    buffer,
                    content_width,
                    wrap_enabled,
                    visible_rows,
                    cursor_line,
                );
                self.scroll_top = new_top;
                return;
            }
            line += 1;
        }
    }

    /// Compute how many screen rows ahead of `cursor_line` we need to show,
    /// then pick the first buffer line that fits above it.
    fn scroll_up_to_fit(
        &self,
        buffer: &TextBuffer,
        content_width: usize,
        wrap_enabled: bool,
        visible_rows: usize,
        cursor_line: usize,
    ) -> usize {
        let mut scroll = cursor_line;
        let mut needed = 0;
        while scroll > 1 {
            let text = buffer.get_line(scroll - 1);
            let w = wrapped_rows(&text, content_width, wrap_enabled);
            if needed + w > visible_rows {
                break;
            }
            needed += w;
            scroll -= 1;
        }
        scroll.max(1)
    }

    // ── Cursor → screen coordinates ──

    /// Convert a buffer (line, col) into screen (row, col).
    #[allow(clippy::too_many_arguments)]
    fn cursor_to_screen(
        &self,
        visual_lines: &[VisualLine],
        cursor_line: usize,
        cursor_col: usize,
        line_number_prefix: usize,
        _content_width: usize,
        _wrap: bool,
        _buffer: &TextBuffer,
    ) -> (usize, usize) {
        // Exact match: find visual line for this buffer line & segment
        for (row, vl) in visual_lines.iter().enumerate() {
            if vl.buffer_line == cursor_line {
                let seg_start = vl.segment_start_col;
                let seg_len = vl.styled_chars.len();
                let col_in_seg = if seg_len > 0 {
                    cursor_col
                        .saturating_sub(seg_start)
                        .min(seg_len.saturating_sub(1))
                } else {
                    0
                };
                return (row, line_number_prefix + col_in_seg);
            }
        }

        // Cursor line not in viewport — estimate using first line
        if let Some(first) = visual_lines.first() {
            if first.buffer_line == cursor_line {
                return (
                    0,
                    line_number_prefix + cursor_col.min(_content_width.saturating_sub(1)),
                );
            }
        }

        (0, line_number_prefix)
    }

    // ── Status line ──

    /// Render the status/command/confirmation line.
    fn render_status(&mut self, row: usize, cols: usize, state: &EditorState, style: CellStyle) {
        let text = if let Some(ref msg) = state.last_message {
            format!(" {} ", msg)
        } else if state.has_confirmation() {
            state
                .confirmation_prompt
                .as_ref()
                .map(|c| c.message.clone())
                .unwrap_or_default()
        } else if state.mode == Mode::Command {
            format!(":{}", state.command_buffer)
        } else {
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

            let file = state
                .file_path
                .as_ref()
                .map(|p| p.to_str().unwrap_or("[Invalid Path]"))
                .unwrap_or("[No Name]");

            if state.mode == Mode::Replace {
                format!(
                    "--{}-- {} {}:{}",
                    mode_label,
                    file,
                    state.cursor.line,
                    state.cursor.col + 1,
                )
            } else {
                let dirty = if state.dirty { "[+]" } else { "" };
                format!(
                    "--{}-- {} {} {}:{}",
                    mode_label,
                    file,
                    dirty,
                    state.cursor.line,
                    state.cursor.col + 1,
                )
            }
        };

        // Clear the whole status row with status style
        for col in 0..cols {
            self.screen.set_cell(row, col, ' ', style);
        }

        // Write text
        for (col, ch) in text.chars().enumerate() {
            if col < cols {
                self.screen.set_cell(row, col, ch, style);
            }
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Free helpers ──

/// Compute how many screen rows a line occupies when wrapped.
fn wrapped_rows(text: &str, max_width: usize, wrap: bool) -> usize {
    if !wrap || max_width == 0 {
        return 1;
    }
    let char_count = text.chars().count();
    if char_count == 0 {
        return 1;
    }
    char_count.div_ceil(max_width)
}

/// Split styled characters into wrapped segments at word boundaries.
///
/// When `wrap` is false, returns a single segment.
/// When `wrap` is true, splits at `max_width` preferring whitespace breaks.
fn wrap_styled(
    styled_chars: &[(char, CellStyle)],
    max_width: usize,
    wrap: bool,
) -> Vec<Vec<(char, CellStyle)>> {
    if !wrap || styled_chars.len() <= max_width {
        return vec![styled_chars.to_vec()];
    }

    let mut result: Vec<Vec<(char, CellStyle)>> = Vec::new();
    let mut start = 0;
    let len = styled_chars.len();

    while start < len {
        if start + max_width >= len {
            result.push(styled_chars[start..].to_vec());
            break;
        }

        let end = start + max_width;

        // Try to break at word boundary (whitespace), scanning backward
        let break_point = (start..end)
            .rev()
            .find(|&i| i > start && styled_chars[i].0.is_whitespace())
            .map(|i| i + 1) // include the space at end of segment
            .unwrap_or(end); // no space found — force break

        result.push(styled_chars[start..break_point].to_vec());
        start = break_point;
    }

    result
}
