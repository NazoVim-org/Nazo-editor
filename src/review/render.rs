use crate::review::layout::{compute_review_layout, PanelRect};
use crate::review::state::{
    DiffFile, DiffLineKind, DiffStyle, FileStatus, ReviewState, ReviewSubMode,
};
use crate::screen::{CellStyle, ScreenBuffer};
use crossterm::style::Color;
use std::path::PathBuf;

// ── Colour palette for review mode ──────────────────────────────────

/// Dark green background for added lines
const ADD_BG: Color = Color::AnsiValue(22);
/// Green foreground for `+` marker
const ADD_FG: Color = Color::AnsiValue(10);
/// Dark red background for deleted lines
const DEL_BG: Color = Color::AnsiValue(52);
/// Red foreground for `-` marker
const DEL_FG: Color = Color::AnsiValue(9);
/// Cyan-ish foreground for hunk headers
const HUNK_HEADER_FG: Color = Color::AnsiValue(33);
/// Selected file/hunk highlight
const SELECTED_BG: Color = Color::AnsiValue(236);
/// Status bar background
const STATUS_BG: Color = Color::AnsiValue(236);
/// Mode indicator background (blue)
const MODE_BG: Color = Color::AnsiValue(21);
/// Added file status
const STATUS_A: Color = Color::AnsiValue(10);
/// Modified file status
const STATUS_M: Color = Color::AnsiValue(11);
/// Deleted file status
const STATUS_D: Color = Color::AnsiValue(9);
/// Renamed file status
const STATUS_R: Color = Color::AnsiValue(13);
/// Dim foreground for secondary info
const DIM_FG: Color = Color::AnsiValue(245);
/// Separator between file list and diff
const SEPARATOR_FG: Color = Color::AnsiValue(240);

// ── Main entry point ────────────────────────────────────────────────

/// Render the review mode UI into the screen buffer.
///
/// Call this from `Renderer::render()` when review state is active.
pub fn render_review(screen: &mut ScreenBuffer, review: &ReviewState, rows: usize, cols: usize) {
    let layout = compute_review_layout(rows, cols, review.file_list_width);

    // Clear the screen
    for r in 0..layout.status_row {
        for c in 0..cols {
            screen.set_cell(r, c, ' ', CellStyle::default());
        }
    }

    match review.sub_mode {
        ReviewSubMode::FileList | ReviewSubMode::DiffView => {
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
        }
        ReviewSubMode::Preview => {
            render_file_preview(
                screen,
                review,
                PanelRect {
                    row_start: 0,
                    row_end: layout.status_row,
                    col_start: 0,
                    col_end: cols,
                },
            );
        }
        ReviewSubMode::Help => {
            // Still render the background content dimmed
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
            render_help_overlay(screen, rows, cols);
        }
        ReviewSubMode::CommentInput => {
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
        }
    }

    render_status_bar(screen, review, layout.status_row, cols);
}

// ── File list panel ─────────────────────────────────────────────────

fn render_file_list(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    if rect.width() < 4 || rect.height() < 1 {
        return;
    }

    let title_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    let selected_style = CellStyle {
        bg: SELECTED_BG,
        ..CellStyle::default()
    };

    // Title
    let title = " Files ";
    for (i, ch) in title.chars().enumerate() {
        if i < rect.width() {
            screen.set_cell(rect.row_start, rect.col_start + i, ch, title_style);
        }
    }

    // Separator line under title
    let sep_char = '─';
    for c in rect.col_start..rect.col_end {
        screen.set_cell(
            rect.row_start + 1,
            c,
            sep_char,
            CellStyle {
                fg: SEPARATOR_FG,
                ..CellStyle::default()
            },
        );
    }

    // File entries
    let max_visible = rect.height().saturating_sub(3); // title + sep + count
    for i in 0..review.files.len().min(max_visible) {
        let row = rect.row_start + 2 + i;
        let file = &review.files[i];
        let is_selected = i == review.selected_file;

        // Background for selected item
        if is_selected {
            for c in rect.col_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }

        // Status icon
        let status_fg = status_color(file.status);
        let status_str = match file.status {
            FileStatus::Added => "A ",
            FileStatus::Modified => "M ",
            FileStatus::Deleted => "D ",
            FileStatus::Renamed => "R ",
        };
        let status_style = CellStyle {
            fg: status_fg,
            bg: if is_selected {
                SELECTED_BG
            } else {
                Color::Reset
            },
            bold: true,
            ..CellStyle::default()
        };
        for (j, ch) in status_str.chars().enumerate() {
            if rect.col_start + j < rect.col_end {
                screen.set_cell(row, rect.col_start + j, ch, status_style);
            }
        }

        // Filename
        let name_start = rect.col_start + 2;
        let name_width = rect.width().saturating_sub(2);
        let display_name = short_filename(&file.new_path, name_width);
        let name_style = CellStyle {
            bg: if is_selected {
                SELECTED_BG
            } else {
                Color::Reset
            },
            ..CellStyle::default()
        };
        for (j, ch) in display_name.chars().enumerate() {
            if name_start + j < rect.col_end {
                screen.set_cell(row, name_start + j, ch, name_style);
            }
        }

        // Fill remaining with spaces if selected
        if is_selected {
            let fill_start = name_start + display_name.chars().count();
            for c in fill_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }
    }

    // File count at bottom
    let count_row = rect.row_end.saturating_sub(1);
    let count_text = format!(" {}/{} ", review.selected_file + 1, review.files.len());
    let count_style = CellStyle {
        fg: DIM_FG,
        ..CellStyle::default()
    };
    for (j, ch) in count_text.chars().enumerate() {
        if rect.col_start + j < rect.col_end {
            screen.set_cell(count_row, rect.col_start + j, ch, count_style);
        }
    }
}

// ── Separator ───────────────────────────────────────────────────────

fn render_separator(screen: &mut ScreenBuffer, col: usize, max_row: usize, _cols: usize) {
    let sep_style = CellStyle {
        fg: SEPARATOR_FG,
        ..CellStyle::default()
    };
    for r in 0..max_row {
        screen.set_cell(r, col, '│', sep_style);
    }
}

// ── Diff view panel ─────────────────────────────────────────────────

fn render_diff_view(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    if rect.width() < 4 || rect.height() < 1 {
        return;
    }

    let file = match review.files.get(review.selected_file) {
        Some(f) => f,
        None => return,
    };

    match review.diff_style {
        DiffStyle::Unified => render_unified_diff(screen, review, file, rect),
        DiffStyle::SideBySide => render_side_by_side_diff(screen, review, file, rect),
    }
}

// ── Unified diff rendering ──────────────────────────────────────────

fn render_unified_diff(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    file: &DiffFile,
    rect: PanelRect,
) {
    // File header
    let header = format!("--- a/{}", file.old_path);
    let header_style = CellStyle {
        fg: DIM_FG,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &header,
        header_style,
    );

    let header2 = format!("+++ b/{}", file.new_path);
    set_string_styled(
        screen,
        rect.row_start + 1,
        rect.col_start,
        &header2,
        header_style,
    );

    // Collect all lines from all hunks with their hunk headers
    let mut display_lines: Vec<DisplayLine> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        let is_selected_hunk = hunk_idx == review.selected_hunk;

        // Hunk header with staged indicator
        display_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: hunk.header.clone(),
            is_selected: is_selected_hunk,
            staged: hunk.staged,
        });

        for line in &hunk.lines {
            display_lines.push(DisplayLine {
                kind: match line.kind {
                    DiffLineKind::Add => DisplayLineKind::Added,
                    DiffLineKind::Delete => DisplayLineKind::Deleted,
                    DiffLineKind::Context => DisplayLineKind::Context,
                },
                content: line.content.clone(),
                is_selected: is_selected_hunk,
                staged: hunk.staged,
            });
        }
    }

    // Render visible lines
    let scroll = review.scroll.scroll_top;
    let max_visible = rect.height().saturating_sub(2); // 2 lines for headers
    let content_width = rect.width();

    for i in 0..max_visible {
        let idx = scroll + i;
        if idx >= display_lines.len() {
            break;
        }
        let row = rect.row_start + 2 + i;
        let dl = &display_lines[idx];

        match dl.kind {
            DisplayLineKind::HunkHeader => {
                let staged_indicator = if dl.staged { "[S] " } else { "[ ] " };
                let full_content = format!("{}{}", staged_indicator, dl.content);
                let style = CellStyle {
                    fg: HUNK_HEADER_FG,
                    bg: if dl.is_selected {
                        SELECTED_BG
                    } else {
                        Color::Reset
                    },
                    bold: true,
                    ..CellStyle::default()
                };
                let truncated = truncate(&full_content, content_width);
                set_string_styled(screen, row, rect.col_start, &truncated, style);
                // Fill remaining
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + display_width(&truncated),
                    rect.col_end,
                    style,
                );
            }
            DisplayLineKind::Added => {
                let prefix_style = CellStyle {
                    fg: ADD_FG,
                    bg: if dl.is_selected { SELECTED_BG } else { ADD_BG },
                    bold: true,
                    ..CellStyle::default()
                };
                let content_style = CellStyle {
                    bg: if dl.is_selected { SELECTED_BG } else { ADD_BG },
                    ..CellStyle::default()
                };
                screen.set_cell(row, rect.col_start, '+', prefix_style);
                let content = truncate(&dl.content, content_width.saturating_sub(1));
                set_string_styled(screen, row, rect.col_start + 1, &content, content_style);
                let fill_bg = if dl.is_selected { SELECTED_BG } else { ADD_BG };
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + 1 + display_width(&content),
                    rect.col_end,
                    CellStyle {
                        bg: fill_bg,
                        ..CellStyle::default()
                    },
                );
            }
            DisplayLineKind::Deleted => {
                let prefix_style = CellStyle {
                    fg: DEL_FG,
                    bg: if dl.is_selected { SELECTED_BG } else { DEL_BG },
                    bold: true,
                    ..CellStyle::default()
                };
                let content_style = CellStyle {
                    bg: if dl.is_selected { SELECTED_BG } else { DEL_BG },
                    ..CellStyle::default()
                };
                screen.set_cell(row, rect.col_start, '-', prefix_style);
                let content = truncate(&dl.content, content_width.saturating_sub(1));
                set_string_styled(screen, row, rect.col_start + 1, &content, content_style);
                let fill_bg = if dl.is_selected { SELECTED_BG } else { DEL_BG };
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + 1 + display_width(&content),
                    rect.col_end,
                    CellStyle {
                        bg: fill_bg,
                        ..CellStyle::default()
                    },
                );
            }
            DisplayLineKind::Context => {
                let style = CellStyle {
                    bg: if dl.is_selected {
                        SELECTED_BG
                    } else {
                        Color::Reset
                    },
                    ..CellStyle::default()
                };
                screen.set_cell(row, rect.col_start, ' ', style);
                let content = truncate(&dl.content, content_width.saturating_sub(1));
                set_string_styled(screen, row, rect.col_start + 1, &content, style);
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + 1 + display_width(&content),
                    rect.col_end,
                    style,
                );
            }
        }
    }

    // No changes indicator
    if display_lines.is_empty() {
        let msg = " (no changes) ";
        let style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        set_string_styled(screen, rect.row_start + 2, rect.col_start, msg, style);
    }
}

// ── Side-by-side diff rendering ─────────────────────────────────────

fn render_side_by_side_diff(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    file: &DiffFile,
    rect: PanelRect,
) {
    // Side-by-side uses half the width for old (left) and half for new (right)
    let half = rect.width() / 2;
    let left_rect = PanelRect {
        col_start: rect.col_start,
        col_end: rect.col_start + half,
        ..rect
    };
    let right_rect = PanelRect {
        col_start: rect.col_start + half,
        col_end: rect.col_end,
        ..rect
    };

    // Title row
    let title_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        left_rect.col_start,
        " OLD ",
        title_style,
    );
    set_string_styled(
        screen,
        rect.row_start,
        right_rect.col_start,
        " NEW ",
        title_style,
    );

    let sep_style = CellStyle {
        fg: SEPARATOR_FG,
        ..CellStyle::default()
    };
    let sep_col = rect.col_start + half;
    for r in rect.row_start..rect.row_end {
        screen.set_cell(r, sep_col, '│', sep_style);
    }

    // Build side-by-side lines from hunks
    // For simplicity, show old on left and new on right, aligned
    let mut left_lines: Vec<DisplayLine> = Vec::new();
    let mut right_lines: Vec<DisplayLine> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        let is_selected = hunk_idx == review.selected_hunk;

        let staged_indicator = if hunk.staged { "[S] " } else { "    " };
        left_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: format!("{}@@ -{}", staged_indicator, hunk.old_start),
            is_selected,
            staged: hunk.staged,
        });
        right_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: format!("{}@@ +{}", staged_indicator, hunk.new_start),
            is_selected,
            staged: hunk.staged,
        });

        for line in &hunk.lines {
            match line.kind {
                DiffLineKind::Add => {
                    // Only in new
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: String::new(),
                        is_selected,
                        staged: hunk.staged,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Added,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                    });
                }
                DiffLineKind::Delete => {
                    // Only in old
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Deleted,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: String::new(),
                        is_selected,
                        staged: hunk.staged,
                    });
                }
                DiffLineKind::Context => {
                    // Both
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                    });
                }
            }
        }
    }

    let max_visible = rect.height().saturating_sub(1);
    let scroll = review.scroll.scroll_top;

    for i in 0..max_visible {
        let idx = scroll + i;
        let row = rect.row_start + 1 + i;

        if idx < left_lines.len() {
            render_side_line(screen, row, left_rect, &left_lines[idx], false);
        }
        if idx < right_lines.len() {
            render_side_line(screen, row, right_rect, &right_lines[idx], true);
        }
    }
}

fn render_side_line(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: PanelRect,
    dl: &DisplayLine,
    _is_right: bool,
) {
    let content_width = rect.width();
    if content_width < 1 {
        return;
    }

    match dl.kind {
        DisplayLineKind::HunkHeader => {
            let style = CellStyle {
                fg: HUNK_HEADER_FG,
                bg: if dl.is_selected {
                    SELECTED_BG
                } else {
                    Color::Reset
                },
                bold: true,
                ..CellStyle::default()
            };
            let truncated = truncate(&dl.content, content_width);
            set_string_styled(screen, row, rect.col_start, &truncated, style);
        }
        DisplayLineKind::Added => {
            let style = CellStyle {
                bg: if dl.is_selected { SELECTED_BG } else { ADD_BG },
                ..CellStyle::default()
            };
            if !dl.content.is_empty() {
                let truncated = truncate(&dl.content, content_width.saturating_sub(1));
                screen.set_cell(
                    row,
                    rect.col_start,
                    '+',
                    CellStyle {
                        fg: ADD_FG,
                        bg: if dl.is_selected { SELECTED_BG } else { ADD_BG },
                        bold: true,
                        ..CellStyle::default()
                    },
                );
                set_string_styled(screen, row, rect.col_start + 1, &truncated, style);
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + 1 + display_width(&truncated),
                    rect.col_end,
                    style,
                );
            } else {
                fill_to_end(screen, row, rect.col_start, rect.col_end, style);
            }
        }
        DisplayLineKind::Deleted => {
            let style = CellStyle {
                bg: if dl.is_selected { SELECTED_BG } else { DEL_BG },
                ..CellStyle::default()
            };
            if !dl.content.is_empty() {
                let truncated = truncate(&dl.content, content_width.saturating_sub(1));
                screen.set_cell(
                    row,
                    rect.col_start,
                    '-',
                    CellStyle {
                        fg: DEL_FG,
                        bg: if dl.is_selected { SELECTED_BG } else { DEL_BG },
                        bold: true,
                        ..CellStyle::default()
                    },
                );
                set_string_styled(screen, row, rect.col_start + 1, &truncated, style);
                fill_to_end(
                    screen,
                    row,
                    rect.col_start + 1 + display_width(&truncated),
                    rect.col_end,
                    style,
                );
            } else {
                fill_to_end(screen, row, rect.col_start, rect.col_end, style);
            }
        }
        DisplayLineKind::Context => {
            let style = CellStyle {
                bg: if dl.is_selected {
                    SELECTED_BG
                } else {
                    Color::Reset
                },
                ..CellStyle::default()
            };
            if !dl.content.is_empty() {
                let truncated = truncate(&dl.content, content_width);
                set_string_styled(screen, row, rect.col_start, &truncated, style);
            }
        }
    }
}

// ── File preview ────────────────────────────────────────────────────

fn render_file_preview(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    let file = match review.files.get(review.selected_file) {
        Some(f) => f,
        None => return,
    };

    let header = format!(" Preview: {} ", file.new_path);
    let header_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &header,
        header_style,
    );

    let scroll = review.scroll.scroll_top;
    let max_visible = rect.height().saturating_sub(1);

    // Try to read the actual file from disk
    let lines = read_file_lines(review, &file.new_path);

    if !lines.is_empty() {
        // Render actual file content
        for i in 0..max_visible {
            let idx = scroll + i;
            if idx >= lines.len() {
                break;
            }
            let row = rect.row_start + 1 + i;
            let truncated = truncate(&lines[idx], rect.width());
            let style = CellStyle {
                fg: Color::AnsiValue(255),
                ..CellStyle::default()
            };
            set_string_styled(screen, row, rect.col_start, &truncated, style);
        }

        // Show line count
        let count_text = format!(" {} lines ", lines.len());
        let count_style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        let count_row = rect.row_end.saturating_sub(1);
        set_string_styled(screen, count_row, rect.col_start, &count_text, count_style);
    } else {
        // Fallback: show diff lines as pseudo-file
        let mut all_lines: Vec<String> = Vec::new();
        for hunk in &file.hunks {
            for line in &hunk.lines {
                let clean = line.content.trim_start_matches('+').trim_start_matches('-');
                all_lines.push(clean.to_string());
            }
        }
        for i in 0..max_visible {
            let idx = scroll + i;
            if idx >= all_lines.len() {
                break;
            }
            let row = rect.row_start + 1 + i;
            let truncated = truncate(&all_lines[idx], rect.width());
            set_string_styled(
                screen,
                row,
                rect.col_start,
                &truncated,
                CellStyle::default(),
            );
        }
    }
}

// ── Help overlay ────────────────────────────────────────────────────

fn render_help_overlay(screen: &mut ScreenBuffer, rows: usize, cols: usize) {
    let help_lines = vec![
        " ╔══════════════════════════════════════════╗ ",
        " ║         Review Mode Help                 ║ ",
        " ╠══════════════════════════════════════════╣ ",
        " ║  q   Exit review mode                    ║ ",
        " ║  ?   Toggle this help                    ║ ",
        " ║  Esc Back to previous panel / exit       ║ ",
        " ║                                          ║ ",
        " ║  File List:                              ║ ",
        " ║    j/k  Navigate files                   ║ ",
        " ║    Enter Open diff view                  ║ ",
        " ║                                          ║ ",
        " ║  Diff View:                              ║ ",
        " ║    j/k  Scroll lines                     ║ ",
        " ║    [/]  Previous/next hunk               ║ ",
        " ║    Enter Toggle hunk stage (git apply)   ║ ",
        " ║    o/p   Open file preview               ║ ",
        " ║    d    Toggle diff style (unified/sbs)  ║ ",
        " ║                                          ║ ",
        " ║  Diff Style:                             ║ ",
        " ║    Unified / Side-by-Side (press d)      ║ ",
        " ╚══════════════════════════════════════════╝ ",
    ];

    let box_width = help_lines.iter().map(|l| l.len()).max().unwrap_or(44);
    let start_row = (rows.saturating_sub(help_lines.len())) / 2;
    let start_col = (cols.saturating_sub(box_width)) / 2;

    // Dim background behind help
    let dim_style = CellStyle {
        bg: Color::AnsiValue(234),
        ..CellStyle::default()
    };
    for r in start_row..start_row + help_lines.len() {
        for c in start_col..start_col + box_width {
            screen.set_cell(r, c, ' ', dim_style);
        }
    }

    for (i, line) in help_lines.iter().enumerate() {
        let row = start_row + i;
        for (j, ch) in line.chars().enumerate() {
            let col = start_col + j;
            if col < cols {
                let fg = if ch == '║'
                    || ch == '╔'
                    || ch == '╗'
                    || ch == '╚'
                    || ch == '╝'
                    || ch == '╠'
                    || ch == '╣'
                {
                    Color::AnsiValue(33)
                } else if ch == 'q'
                    || ch == '?'
                    || ch == 'j'
                    || ch == 'k'
                    || ch == '['
                    || ch == ']'
                    || ch == 'd'
                {
                    Color::AnsiValue(11)
                } else {
                    Color::AnsiValue(255)
                };
                let style = CellStyle {
                    fg,
                    bg: Color::AnsiValue(234),
                    ..CellStyle::default()
                };
                screen.set_cell(row, col, ch, style);
            }
        }
    }
}

// ── Status bar ──────────────────────────────────────────────────────

fn render_status_bar(screen: &mut ScreenBuffer, review: &ReviewState, row: usize, cols: usize) {
    // Background for entire status row
    for c in 0..cols {
        screen.set_cell(
            row,
            c,
            ' ',
            CellStyle {
                bg: STATUS_BG,
                ..CellStyle::default()
            },
        );
    }

    // Mode indicator
    let mode_text = " REVIEW ";
    let mode_style = CellStyle {
        fg: Color::White,
        bg: MODE_BG,
        bold: true,
        ..CellStyle::default()
    };
    for (i, ch) in mode_text.chars().enumerate() {
        if i < cols {
            screen.set_cell(row, i, ch, mode_style);
        }
    }

    // Info text
    let file = review.files.get(review.selected_file);
    let file_name = file.map(|f| f.new_path.as_str()).unwrap_or("");
    let changes = review.total_changes();
    let info_text = format!(" {}  |  {} changes  |  {}", file_name, changes, review.args,);
    let info_style = CellStyle {
        fg: Color::White,
        bg: STATUS_BG,
        ..CellStyle::default()
    };
    let info_start = mode_text.len() + 1;
    for (i, ch) in info_text.chars().enumerate() {
        let col = info_start + i;
        if col < cols {
            screen.set_cell(row, col, ch, info_style);
        }
    }

    // Diff style indicator on the right
    let style_text = match review.diff_style {
        DiffStyle::Unified => " unified ",
        DiffStyle::SideBySide => " side-by-side ",
    };
    let style_start = cols.saturating_sub(style_text.len());
    let style_style = CellStyle {
        fg: DIM_FG,
        bg: STATUS_BG,
        ..CellStyle::default()
    };
    for (i, ch) in style_text.chars().enumerate() {
        let col = style_start + i;
        if col < cols {
            screen.set_cell(row, col, ch, style_style);
        }
    }
}

// ── Helper types and functions ──────────────────────────────────────

#[derive(Debug)]
enum DisplayLineKind {
    HunkHeader,
    Added,
    Deleted,
    Context,
}

#[derive(Debug)]
struct DisplayLine {
    kind: DisplayLineKind,
    content: String,
    is_selected: bool,
    /// Whether the hunk this line belongs to is staged
    staged: bool,
}

fn status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Added => STATUS_A,
        FileStatus::Modified => STATUS_M,
        FileStatus::Deleted => STATUS_D,
        FileStatus::Renamed => STATUS_R,
    }
}

fn short_filename(path: &str, max_width: usize) -> String {
    if path.len() <= max_width {
        return path.to_string();
    }
    if max_width < 5 {
        return path.chars().take(max_width).collect();
    }
    let take = max_width.saturating_sub(3);
    let mut result: String = path
        .chars()
        .rev()
        .take(take)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    result = format!("...{}", result);
    result
}

/// Display width of a string (using char count as approximation for now)
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Truncate a string to `max_width` display characters.
fn truncate(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_width {
        s.to_string()
    } else {
        chars.iter().take(max_width).collect()
    }
}

/// Write a plain string at a given position.
fn set_string_styled(
    screen: &mut ScreenBuffer,
    row: usize,
    col: usize,
    text: &str,
    style: CellStyle,
) {
    for (i, ch) in text.chars().enumerate() {
        screen.set_cell(row, col + i, ch, style);
    }
}

/// Fill from `start_col` to `end_col` with spaces using the given style.
fn fill_to_end(
    screen: &mut ScreenBuffer,
    row: usize,
    start_col: usize,
    end_col: usize,
    style: CellStyle,
) {
    for c in start_col..end_col {
        screen.set_cell(row, c, ' ', style);
    }
}

/// Read a file from disk, using `project_root` to resolve the path.
/// Returns an empty vec on error (file not found, not in git repo, etc.).
fn read_file_lines(review: &ReviewState, file_path: &str) -> Vec<String> {
    let base = match &review.project_root {
        Some(root) => root.clone(),
        None => return Vec::new(),
    };
    let full_path: PathBuf = [base, PathBuf::from(file_path)].iter().collect();
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content.lines().map(|l| l.to_string()).collect()
}
