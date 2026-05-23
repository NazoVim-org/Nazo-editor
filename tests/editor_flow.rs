use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode};

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
