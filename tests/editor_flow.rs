use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode, PendingOperator, VisualType};

fn test_vim_editor(text: &str) -> Editor {
    let mut editor = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    editor.set_buffer_for_test(text);
    editor
}

#[tokio::test]
async fn vim_input_flow_and_boundaries() {
    let mut editor = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    editor.set_buffer_for_test("abc\n");

    editor
        .handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;

    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "abc\n");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);

    editor.set_buffer_for_test("\n");
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "\n");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn emacs_input_flow_and_boundaries() {
    let mut editor = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
    editor.set_buffer_for_test("abc\nxyz");

    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('n'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('p'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('s'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('k'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;

    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "abc\nxyz");
    assert_eq!((line, col), (1, 1));
    assert_eq!(mode, Mode::Normal);

    editor.set_buffer_for_test("x");
    editor
        .handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn vim_redo_restores_after_undo() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // dd to delete line 1
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n", "after dd");

    // 'u' undo
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello\nworld\n", "after undo");

    // Ctrl-R redo
    ed.handle_key_for_test(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n", "after redo");
}

#[tokio::test]
async fn vim_page_down_moves_cursor_down() {
    let mut ed = test_vim_editor("line1\nline2\nline3\nline4\nline5\n");
    assert_eq!(ed.snapshot_for_test().1, 1, "starts at line 1");

    // Ctrl-F (page down) should move cursor down
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert!(line > 1, "page down moved cursor to line {}", line);
}

#[tokio::test]
async fn vim_page_up_after_page_down_returns() {
    let mut ed = test_vim_editor("line1\nline2\nline3\nline4\nline5");
    // Go to bottom
    ed.handle_key_for_test(KeyCode::Char('G'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 5, "G goes to last line (5)");

    // Ctrl-B (page up) should move cursor up
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert!(line < 5, "page up moved cursor to line {}", line);
}

#[tokio::test]
async fn vim_delete_inner_word() {
    let mut ed = test_vim_editor("hello world\n");
    // cursor is at (1,0) on 'h'
    // diw  → delete inner word
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "diw deleted 'hello'");
    assert_eq!((line, col), (1, 0), "cursor at start");
}

#[tokio::test]
async fn vim_undo_delete_inner_word() {
    let mut ed = test_vim_editor("hello world\n");
    // diw
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "diw before undo");

    // u → undo
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "undo restored original");
}

#[tokio::test]
async fn vim_visual_yank() {
    let mut ed = test_vim_editor("abcdef\n");
    // vllly   → visual select "abc" then yank
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    // move right twice to select "abc"
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    // yank
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;
    // buffer unchanged
    let (buf, _line, _col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abcdef\n", "buffer unchanged after yank");
    assert_eq!(mode, Mode::Normal, "back to normal after yank");
}

#[tokio::test]
async fn vim_visual_delete() {
    let mut ed = test_vim_editor("abcdef\n");
    // vllld   → visual select "abc" then delete
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _line, _col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "def\n", "visual delete removed 'abc'");
    assert_eq!(mode, Mode::Normal, "back to normal after delete");
}

#[tokio::test]
async fn vim_insert_undo_removes_inserted_text() {
    let mut ed = test_vim_editor("\n");
    // i → insert "xyz" → Esc → u
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    for ch in "xyz".chars() {
        ed.handle_key_for_test(KeyCode::Char(ch), KeyModifiers::NONE)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "xyz\n", "text inserted");

    // 'u' should remove "xyz"
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\n", "undo removed 'xyz'");
}

#[tokio::test]
async fn vim_insert_enter_undo() {
    let mut ed = test_vim_editor("ab\n");
    // Move to 'b', insert newline, Esc, undo
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a\nb\n", "newline inserted after 'a'");

    // 'u' should remove the newline
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ab\n", "undo removed newline");
}

// ── Vim word movement tests ─────────────────────────────────

#[tokio::test]
async fn vim_w_word_forward() {
    let mut ed = test_vim_editor("hello world foo\n");
    assert_eq!(ed.snapshot_for_test().2, 0);
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "w moved to space after 'hello'");
}

#[tokio::test]
async fn vim_b_word_backward() {
    let mut ed = test_vim_editor("hello world\n");
    ed.state.cursor.col = 8;
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "b moved to start of 'world'");
}

#[tokio::test]
async fn vim_e_word_end() {
    let mut ed = test_vim_editor("hello world\n");
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 4, "e moved to end of 'hello' (index 4 = 'o')");
}

#[tokio::test]
async fn vim_dollar_moves_to_line_end() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('$'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 4, "$ moved to last char 'o' (0-indexed 4)");
}

#[tokio::test]
async fn vim_zero_moves_to_column_zero() {
    let mut ed = test_vim_editor("hello\n");
    ed.state.cursor.col = 3;
    ed.handle_key_for_test(KeyCode::Char('0'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.snapshot_for_test().2, 0);
}

// ── Vim visual line mode tests ──────────────────────────────

#[tokio::test]
async fn vim_visual_line_yank() {
    let mut ed = test_vim_editor("abc\ndef\n");
    // V → visual line
    ed.handle_key_for_test(KeyCode::Char('V'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Visual);
    // y → yank
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abc\ndef\n", "buffer unchanged after V y");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn vim_visual_line_delete() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    // V j d → visual line, extend, delete
    ed.handle_key_for_test(KeyCode::Char('V'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "ghi\n", "V j d deleted first two lines");
    assert_eq!(mode, Mode::Normal);
}

// ── Vim C, S, s tests ──────────────────────────────────────

#[tokio::test]
async fn vim_capital_c_deletes_to_end_and_inserts() {
    let mut ed = test_vim_editor("hello world\n");
    ed.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('C'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Insert);
    let line = ed.buffer.get_line(1);
    assert_eq!(line, "hello\n", "C deleted from col 5 to end");
}

#[tokio::test]
async fn vim_capital_s_substitutes_line() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('S'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Insert);
    let line = ed.buffer.get_line(1);
    assert_eq!(line, "\n", "S cleared the line");
}

#[tokio::test]
async fn vim_s_substitutes_char() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('s'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Insert);
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello\n", "s deleted first char");
}

// ── Vim C, D tests ─────────────────────────────────────────

#[tokio::test]
async fn vim_capital_d_deletes_to_end() {
    let mut ed = test_vim_editor("hello world\n");
    ed.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('D'), KeyModifiers::NONE)
        .await;
    let line = ed.buffer.get_line(1);
    assert_eq!(line, "hello\n", "D deleted ' world'");
}

// ── Vim find char tests (f/F/t/T/;/,) ──────────────────────

#[tokio::test]
async fn vim_find_char_f() {
    let mut ed = test_vim_editor("hello world\n");
    // fw: find 'w'
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "f w moves to 'w' at col 6");
}

#[tokio::test]
async fn vim_find_char_till_t() {
    let mut ed = test_vim_editor("hello world\n");
    // tw: till 'w'
    ed.handle_key_for_test(KeyCode::Char('t'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "t w stops before 'w' at col 5");
}

#[tokio::test]
async fn vim_find_repeat_semicolon() {
    let mut ed = test_vim_editor("he.llo wo.rld\n");
    // f. → find first '.'
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('.'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 2, "f. moved to first '.' at col 2");
    // ; → repeat find next '.'
    ed.handle_key_for_test(KeyCode::Char(';'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 9, "; repeated to second '.' at col 9");
}

// ── Vim J join lines ───────────────────────────────────────

#[tokio::test]
async fn vim_join_lines_j() {
    let mut ed = test_vim_editor("hello\nworld\n");
    ed.handle_key_for_test(KeyCode::Char('J'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "J joined lines with space");
}

// ── Vim percent bracket matching ───────────────────────────

#[tokio::test]
async fn vim_percent_matching_bracket() {
    let mut ed = test_vim_editor("(hello)\n");
    // cursor at col 0 '('
    ed.handle_key_for_test(KeyCode::Char('%'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1);
    assert_eq!(col, 6, "% moved to ')' at col 6");
}

// ── Vim gg / G ────────────────────────────────────────────

#[tokio::test]
async fn vim_gg_goes_to_first_line() {
    let mut ed = test_vim_editor("a\nb\nc\n");
    ed.state.cursor.line = 3;
    // gg
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1, "gg went to line 1");
    assert_eq!(col, 0, "gg went to col 0");
}

#[tokio::test]
async fn vim_g_with_count_goes_to_line() {
    let mut ed = test_vim_editor("a\nb\nc\nd\n");
    // 2G → go to line 2
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('G'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 2, "2G went to line 2");
}

// ── Vim ZZ save and quit ───────────────────────────────────

#[tokio::test]
async fn vim_zz_save_and_quit_clean_file() {
    let mut ed = test_vim_editor("hello\n");
    ed.state.file_path = Some(std::path::PathBuf::from("/tmp/zz_test.txt"));
    // Make buffer not dirty
    ed.buffer.reset_clean_state();
    // ZZ
    ed.handle_key_for_test(KeyCode::Char('Z'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('Z'), KeyModifiers::NONE)
        .await;
    assert!(!ed.running, "ZZ quit after clean save");
}

// ── Vim count prefix tests ─────────────────────────────────

#[tokio::test]
async fn vim_count_prefix_with_j() {
    let mut ed = test_vim_editor("a\nb\nc\nd\ne\n");
    // 3j → down 3 lines
    ed.handle_key_for_test(KeyCode::Char('3'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 4, "3j went to line 4");
}

#[tokio::test]
async fn vim_count_prefix_with_dd() {
    let mut ed = test_vim_editor("a\nb\nc\n");
    // 2dd → delete 2 lines
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "c\n", "2dd deleted first 2 lines");
}

// ── Vim indent tests ───────────────────────────────────────

#[tokio::test]
async fn vim_indent_right_shift() {
    let mut ed = test_vim_editor("hello\n");
    // >>
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    let line = ed.buffer.get_line(1);
    assert!(line.starts_with('\t'), ">> indented with tab");
}

// ── Vim visual change ──────────────────────────────────────

#[tokio::test]
async fn vim_visual_change_enters_insert() {
    let mut ed = test_vim_editor("hello world\n");
    // vlllc  → visual select "hel" then change
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('c'), KeyModifiers::NONE)
        .await;
    let (buf, _, col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "lo world\n", "visual change deleted 'hel'");
    assert_eq!(col, 0, "cursor at start");
    assert_eq!(mode, Mode::Insert, "entered insert mode");
}

// ── Vim question mark search ───────────────────────────────

#[tokio::test]
async fn vim_question_mark_enters_backward_search() {
    let mut ed = test_vim_editor("hello world\n");
    ed.handle_key_for_test(KeyCode::Char('?'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Command);
    assert_eq!(ed.state.command_buffer, "?");
}

// ── Emacs M-f / M-b word movement ──────────────────────────

#[tokio::test]
async fn emacs_meta_f_moves_word_forward() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world foo\n");
    // M-f
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "M-f moved to space after 'hello'");
}

#[tokio::test]
async fn emacs_meta_b_moves_word_backward() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.state.cursor.col = 8;
    // M-b
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::ALT)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "M-b moved to start of 'world'");
}

// ── Emacs C-t transpose ────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_t_transpose_chars() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello\n");
    ed.state.cursor.col = 2; // at 'e' and 'l' (col 2 → between 'e' and 'l')
                             // Actually transpose swaps chars before and at cursor
                             // Cursor at col 1: swaps 'h'(0) and 'e'(1)
    ed.state.cursor.col = 1;
    ed.handle_key_for_test(KeyCode::Char('t'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ehllo\n", "C-t transposed 'he' to 'eh'");
    assert_eq!(col, 2, "cursor advanced after transpose");
}

// ── Emacs C-g abort ────────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_g_aborts_command_mode() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.state.mode = Mode::Command;
    ed.state.command_buffer = "/search".to_string();
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.state.mode, Mode::Normal);
    assert!(
        ed.state.command_buffer.is_empty(),
        "C-g cleared command buffer"
    );
}

// ── Emacs C-x u redo ───────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_u_redo() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Make a change, undo it, then C-x u redo
    ed.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-d deleted space");

    // Undo
    ed.handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "C-/ undid deletion");

    // C-x u redo
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-x u redid deletion");
}

// ── Emacs C-y yank from kill ring ──────────────────────────

#[tokio::test]
async fn emacs_ctrl_y_yanks_from_kill_ring() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello\n");
    // Kill "hello"
    ed.handle_key_for_test(KeyCode::Char('k'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\n", "C-k killed line content");

    // Yank back at start
    ed.state.cursor.col = 0;
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello\n", "C-y yanked 'hello' back");
}

// ── Emacs region kill (C-w) ────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_w_kills_region() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Set mark at 0, move cursor to 5, C-w kills region
    ed.handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
        .await;
    for _ in 0..5 {
        ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "C-w killed 'hello'");
}

// ── Emacs M-w copy region ──────────────────────────────────

#[tokio::test]
async fn emacs_meta_w_copies_region() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Set mark at 0, move cursor to 5, M-w copies region
    ed.handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
        .await;
    for _ in 0..5 {
        ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::ALT)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "M-w did not change buffer");
    // kill-ring should have "hello" (checked via yank after)
    ed.state.cursor.col = 0;
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::CONTROL)
        .await;
    let (buf2, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf2, "hellohello world\n", "C-y yanked copied 'hello' back");
}

// ── Keymap switching tests ─────────────────────────────────

#[tokio::test]
async fn switch_from_vim_to_emacs_and_back() {
    let mut ed = test_vim_editor("hello world\n");
    assert!(matches!(ed.state.mode, Mode::Normal));

    // Switch to Emacs
    ed.switch_keymap("emacs");
    // In Emacs mode, M-f should move word forward (not insert 'f')
    let col_before = ed.state.cursor.col;
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    let col_after = ed.state.cursor.col;
    assert!(
        col_after > col_before,
        "M-f moved cursor forward after emacs switch"
    );
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "buffer unchanged after M-f");

    // Switch back to Vim
    ed.switch_keymap("vim");
    ed.state.cursor.col = 0;
    // Vim: 'x' should delete a character
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello world\n", "x deleted first char after vim switch");
}

#[tokio::test]
async fn switch_keymap_resets_pending_state() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    // Start dd with count (partial command)
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    // Switch keymap mid-command — should reset pending count
    ed.switch_keymap("emacs");
    assert!(ed.pending_count.is_none(), "pending_count cleared");
    assert!(
        matches!(ed.pending_op, PendingOperator::Idle),
        "command_state reset"
    );
    assert_eq!(ed.state.mode, Mode::Normal);
}

#[tokio::test]
async fn switch_keymap_via_command_mode() {
    let mut ed = test_vim_editor("hello\n");
    // :keymap emacs in command mode
    ed.handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    for ch in "keymap emacs".chars() {
        ed.handle_key_for_test(KeyCode::Char(ch), KeyModifiers::NONE)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;

    // Now in Emacs mode: M-f should move word forward
    let col_before = ed.state.cursor.col;
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    assert!(
        ed.state.cursor.col > col_before,
        "M-f works after :keymap emacs"
    );
}

#[tokio::test]
async fn switch_keymap_unknown_name_does_not_panic() {
    let mut ed = test_vim_editor("hello\n");
    ed.switch_keymap("unknown_mode");
    // Still in Vim mode — 'x' deletes char
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello\n", "still in vim mode after unknown keymap");
}

#[tokio::test]
async fn vim_c_after_emacs_switch_enters_visual_block() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // Switch to Emacs then back to Vim
    ed.switch_keymap("emacs");
    ed.switch_keymap("vim");

    // Ctrl-v should enter visual block mode
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.state.mode, Mode::Visual);
    assert_eq!(ed.state.visual_type, Some(VisualType::Block));
    // Esc should exit
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    assert_eq!(ed.state.mode, Mode::Normal);
}
