use crate::buffer::TextBuffer;
use crate::highlight::Style;
use crate::screen::{build_styled_chars, CellStyle, ScreenBuffer};
use crate::terminal::Terminal;
use crate::types::{EditorState, MessageLevel, Mode, Window, WindowLayout};
use crossterm::style::Color;
use std::io;

/// Render context for a single window.
struct WindowRenderContext<'a> {
    window: &'a Window,
    active_buffer: &'a TextBuffer,
    inactive_buffers: &'a [(
        TextBuffer,
        crate::undo::UndoManager,
        Option<std::path::PathBuf>,
        crate::types::Position,
    )],
    state: &'a EditorState,
    highlights: &'a [Vec<(Style, String)>],
    is_focused: bool,
    total_cols: usize,
}

/// One visual (display) row produced from wrapping a buffer line.
#[allow(dead_code)]
struct VisualLine {
    /// Buffer line number (1-indexed).
    buffer_line: usize,
    /// Starting buffer column of this visual segment.
    #[allow(dead_code)]
    segment_start_col: usize,
    /// Styled characters to display on this screen row.
    styled_chars: Vec<(char, CellStyle)>,
}

pub struct Renderer {
    /// Screen cell buffer for flicker-free diff-based rendering.
    screen: ScreenBuffer,
    /// Cached show_line_numbers for dirty detection.
    #[allow(dead_code)]
    show_line_numbers: bool,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            screen: ScreenBuffer::new(24, 80),
            show_line_numbers: true,
        }
    }

    /// Main entry point: compose a frame into the screen buffer and emit
    /// minimal ANSI diffs to the terminal.
    ///
    /// `window_layout` contains all windows with their viewports and buffers.
    /// `inactive_buffers` holds buffers not currently in the active window.
    /// `highlights` is the per-line syntect highlight data for the active buffer.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        terminal: &mut Terminal,
        active_buffer: &TextBuffer,
        inactive_buffers: &[(
            TextBuffer,
            crate::undo::UndoManager,
            Option<std::path::PathBuf>,
            crate::types::Position,
        )],
        state: &EditorState,
        window_layout: &WindowLayout,
        highlights: &[Vec<(Style, String)>],
    ) -> io::Result<()> {
        let rows = terminal.rows() as usize;
        let cols = terminal.cols() as usize;

        // ── Resize screen if needed (handles terminal resize) ──
        let size_changed = self.screen.rows != rows || self.screen.cols != cols;
        self.screen.resize(rows, cols);
        if size_changed {
            self.screen.invalidate_all();
        }
        self.screen.clear_all();

        // If no windows, just show empty screen with status bar
        if window_layout.windows.is_empty() {
            self.render_status_bar(rows, cols, state);
            let ansi_output = self.screen.render();
            if !ansi_output.is_empty() {
                terminal.write_raw(ansi_output.as_bytes())?;
            }
            return Ok(());
        }

        // Render each window
        for (idx, window) in window_layout.windows.iter().enumerate() {
            let is_focused = idx == window_layout.focused;
            self.render_window(WindowRenderContext {
                window,
                active_buffer,
                inactive_buffers,
                state,
                highlights,
                is_focused,
                total_cols: cols,
            });
        }

        // Draw window separators
        self.draw_window_separators(window_layout, cols);

        // Render global status bar (last row)
        self.render_status_bar(rows, cols, state);

        // Position hardware cursor at focused window's cursor
        if let Some(focused_window) = window_layout.windows.get(window_layout.focused) {
            self.position_cursor(focused_window, state, cols);
        }

        // Emit frame to terminal
        let ansi_output = self.screen.render();
        if !ansi_output.is_empty() {
            terminal.write_raw(ansi_output.as_bytes())?;
        }

        Ok(())
    }

    /// Render a single window into its viewport.
    fn render_window(&mut self, ctx: WindowRenderContext<'_>) {
        let window = ctx.window;
        let active_buffer = ctx.active_buffer;
        let inactive_buffers = ctx.inactive_buffers;
        let state = ctx.state;
        let highlights = ctx.highlights;
        let _is_focused = ctx.is_focused;
        let total_cols = ctx.total_cols;

        let window_rows = window.row_end.saturating_sub(window.row_start);
        if window_rows == 0 || window.col_end <= window.col_start {
            return;
        }

        // Select the buffer for this window
        let buffer = if window.buf_idx == 0 {
            active_buffer
        } else if window.buf_idx > 0 && window.buf_idx - 1 < inactive_buffers.len() {
            &inactive_buffers[window.buf_idx - 1].0
        } else {
            // Fallback
            active_buffer
        };

        let window_cols = window.col_end.saturating_sub(window.col_start);
        if window_cols == 0 {
            return;
        }

        // Line number metrics for this window
        let show_any_numbers = state.show_line_numbers || state.show_relative_numbers;
        let line_number_width = if show_any_numbers {
            let total = buffer.line_count().max(1);
            total.to_string().len()
        } else {
            0
        };
        let line_number_prefix = if show_any_numbers {
            line_number_width + 2
        } else {
            0
        };
        let content_width = window_cols.saturating_sub(line_number_prefix).max(1);

        // Build visual lines for this window
        let mut visual_lines: Vec<VisualLine> = Vec::new();

        // Build layout starting from window's scroll_top
        let mut buf_line = window.scroll_top.max(1);
        let mut screen_rows_used = 0;
        while screen_rows_used < window_rows && buf_line <= buffer.line_count() {
            let raw_line = buffer.get_line(buf_line);
            let line_highlights = highlights
                .get(buf_line.saturating_sub(1))
                .map(|v| v.as_slice());
            let styled_chars = build_styled_chars(&raw_line, line_highlights);
            let segments = wrap_styled(&styled_chars, content_width, true); // Always wrap in windows

            for (seg_idx, segment) in segments.iter().enumerate() {
                if screen_rows_used >= window_rows {
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

        // Render content rows into screen buffer
        let gray_style = CellStyle {
            fg: Color::AnsiValue(240),
            ..CellStyle::default()
        };

        for (screen_row, vline) in visual_lines.iter().enumerate() {
            let target_row = window.row_start + screen_row;
            if target_row >= window.row_end {
                break;
            }

            // Line number prefix
            if show_any_numbers {
                let display_num = if state.show_relative_numbers {
                    let dist =
                        (vline.buffer_line as isize - window.scroll_top as isize).unsigned_abs();
                    if dist == 0 && state.show_line_numbers {
                        vline.buffer_line
                    } else {
                        dist
                    }
                } else {
                    vline.buffer_line
                };
                let num_str = format!("{:width$}  ", display_num, width = line_number_width);
                for (col, ch) in num_str.chars().enumerate() {
                    let style = if col < line_number_width {
                        gray_style
                    } else {
                        CellStyle::default()
                    };
                    let target_col = window.col_start + col;
                    if target_col < window.col_end {
                        self.screen.set_cell(target_row, target_col, ch, style);
                    }
                }
            }

            // Content
            for (col, (ch, style)) in vline.styled_chars.iter().enumerate() {
                let target_col = window.col_start + line_number_prefix + col;
                if target_col < window.col_end && target_col < total_cols {
                    let mut cell_style = *style;
                    // Highlight current line in focused window
                    if vline.buffer_line == self.get_focused_cursor_line(window) {
                        cell_style.bg = Color::AnsiValue(235);
                    }
                    self.screen
                        .set_cell(target_row, target_col, *ch, cell_style);
                }
            }

            // Fill remainder with spaces
            let last_col = window.col_start + line_number_prefix + vline.styled_chars.len();
            for col in last_col..window.col_end.min(total_cols) {
                self.screen
                    .set_cell(target_row, col, ' ', CellStyle::default());
            }
        }

        // Fill remaining rows with ~ or spaces
        for screen_row in visual_lines.len()..window_rows {
            let target_row = window.row_start + screen_row;
            if target_row >= window.row_end {
                break;
            }
            if show_any_numbers {
                let num_str = format!("{:width$}  ", "~", width = line_number_width);
                for (col, ch) in num_str.chars().enumerate() {
                    let style = if col < line_number_width {
                        gray_style
                    } else {
                        CellStyle::default()
                    };
                    let target_col = window.col_start + col;
                    if target_col < window.col_end {
                        self.screen.set_cell(target_row, target_col, ch, style);
                    }
                }
            }
            for col in (window.col_start + line_number_prefix)..window.col_end.min(total_cols) {
                self.screen
                    .set_cell(target_row, col, ' ', CellStyle::default());
            }
        }
    }

    /// Get the cursor line for the focused window (for line highlighting).
    fn get_focused_cursor_line(&self, window: &Window) -> usize {
        window.cursor.line
    }

    /// Draw vertical/horizontal separators between windows.
    fn draw_window_separators(&mut self, layout: &WindowLayout, total_cols: usize) {
        if layout.windows.len() <= 1 {
            return;
        }

        // Find vertical splits (same row range, adjacent columns)
        for i in 0..layout.windows.len() {
            for j in i + 1..layout.windows.len() {
                let w1 = &layout.windows[i];
                let w2 = &layout.windows[j];

                // Vertical separator: same rows, adjacent columns
                if w1.row_start == w2.row_start && w1.row_end == w2.row_end {
                    if w1.col_end == w2.col_start {
                        // Vertical line at w1.col_end
                        let col = w1.col_end.min(total_cols);
                        for row in w1.row_start..w1.row_end {
                            self.screen.set_cell(row, col, '│', CellStyle::default());
                        }
                    } else if w2.col_end == w1.col_start {
                        let col = w2.col_end.min(total_cols);
                        for row in w1.row_start..w1.row_end {
                            self.screen.set_cell(row, col, '│', CellStyle::default());
                        }
                    }
                }

                // Horizontal separator: same columns, adjacent rows
                if w1.col_start == w2.col_start && w1.col_end == w2.col_end {
                    if w1.row_end == w2.row_start {
                        let row = w1.row_end;
                        for col in w1.col_start..w1.col_end.min(total_cols) {
                            self.screen.set_cell(row, col, '─', CellStyle::default());
                        }
                    } else if w2.row_end == w1.row_start {
                        let row = w2.row_end;
                        for col in w1.col_start..w1.col_end.min(total_cols) {
                            self.screen.set_cell(row, col, '─', CellStyle::default());
                        }
                    }
                }
            }
        }
    }

    /// Render global status bar at the bottom.
    fn render_status_bar(&mut self, rows: usize, cols: usize, state: &EditorState) {
        let status_row = rows.saturating_sub(1);
        let is_show_message = state.last_message.is_some();
        let status_style = if is_show_message {
            let level = state
                .message_history
                .back()
                .map(|m| m.level)
                .unwrap_or(MessageLevel::Info);
            match level {
                MessageLevel::Error => CellStyle {
                    bg: Color::Red,
                    fg: Color::White,
                    bold: true,
                    ..CellStyle::default()
                },
                MessageLevel::Warning => CellStyle {
                    bg: Color::AnsiValue(226),
                    fg: Color::Black,
                    bold: true,
                    ..CellStyle::default()
                },
                MessageLevel::Info => CellStyle {
                    bg: Color::AnsiValue(21),
                    fg: Color::White,
                    bold: true,
                    ..CellStyle::default()
                },
            }
        } else {
            CellStyle {
                bg: Color::AnsiValue(236),
                fg: Color::White,
                ..CellStyle::default()
            }
        };

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
            self.screen.set_cell(status_row, col, ' ', status_style);
        }

        // Write text
        for (col, ch) in text.chars().enumerate() {
            if col < cols {
                self.screen.set_cell(status_row, col, ch, status_style);
            }
        }
    }

    /// Position the hardware cursor at the focused window's cursor position.
    fn position_cursor(&mut self, window: &Window, _state: &EditorState, _total_cols: usize) {
        // Calculate screen position from window's cursor
        // For simplicity, use the window's cursor directly
        // In a full implementation, this would account for wrapping
        let row = window.row_start
            + (window.cursor.line.saturating_sub(window.scroll_top)).min(
                window
                    .row_end
                    .saturating_sub(window.row_start)
                    .saturating_sub(1),
            );
        let col = window.col_start + window.cursor.col;
        self.screen.set_cell(
            row,
            col,
            ' ',
            CellStyle {
                fg: Color::Black,
                bg: Color::White,
                ..CellStyle::default()
            },
        );
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Free helper functions ──

/// Number of wrapped rows a line would occupy.
pub fn wrapped_rows(text: &str, max_width: usize, wrap: bool) -> usize {
    if !wrap {
        return 1;
    }
    let len = text.chars().count();
    if len == 0 {
        return 1;
    }
    len.div_ceil(max_width)
}

/// Split styled chars into screen rows.
pub fn wrap_styled(
    styled_chars: &[(char, CellStyle)],
    max_width: usize,
    wrap: bool,
) -> Vec<Vec<(char, CellStyle)>> {
    if !wrap || styled_chars.is_empty() {
        return vec![styled_chars.to_vec()];
    }
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut width = 0;

    for (ch, style) in styled_chars {
        let ch_width = ch.len_utf8().min(1); // Simplified
        if width + ch_width > max_width && !current.is_empty() {
            rows.push(current);
            current = Vec::new();
            width = 0;
        }
        current.push((*ch, *style));
        width += ch_width;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}
