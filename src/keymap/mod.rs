use crate::editor::Editor;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub mod emacs;
mod vim;

use crossterm::event::{KeyCode, KeyModifiers};
pub use emacs::EmacsKeymap;
pub use vim::VimKeymap;

/// Single-threaded TUI key dispatcher.
///
/// Keymap handlers use interior mutability for their own state (e.g., Emacs
/// prefix keys), so the trait takes `&self`. The returned future borrows the
/// `Editor` but not the keymap handler itself, avoiding `RefCell` borrow
/// issues across `.await` points.
pub trait KeymapHandler {
    fn handle_key<'e>(
        &self,
        editor: &'e mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> Pin<Box<dyn Future<Output = ()> + 'e>>;
}

pub fn create_keymap(keymap: crate::types::Keymap) -> Rc<RefCell<dyn KeymapHandler>> {
    match keymap {
        crate::types::Keymap::Vim => Rc::new(RefCell::new(VimKeymap::new())),
        crate::types::Keymap::Emacs => Rc::new(RefCell::new(EmacsKeymap::new())),
    }
}
