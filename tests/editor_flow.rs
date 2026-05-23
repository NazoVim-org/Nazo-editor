use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode};

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
