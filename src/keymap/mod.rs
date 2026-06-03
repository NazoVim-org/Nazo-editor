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
/// NOT Send/Sync by design: keymap handlers use interior mutability patterns
/// (`Rc<RefCell<...>>`) and are bound to the main event loop.
pub trait KeymapHandler {
    fn handle_key<'a>(
        &'a mut self,
        editor: &'a mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
}

pub fn create_keymap(keymap: crate::types::Keymap) -> Rc<RefCell<dyn KeymapHandler>> {
    match keymap {
        crate::types::Keymap::Vim => Rc::new(RefCell::new(VimKeymap::new())),
        crate::types::Keymap::Emacs => Rc::new(RefCell::new(EmacsKeymap::new())),
    }
}
