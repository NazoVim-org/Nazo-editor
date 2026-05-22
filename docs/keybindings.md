# Keybinding Specification

This document tracks keybinding behavior implemented in the current codebase.

## Vim keymap

- `vim` mode can be selected from CLI (`ivim vim`), but the keymap handler in `src/keymap/vim.rs` is currently a stub and does not implement key handling logic.

## Emacs keymap

The following bindings are implemented in `src/keymap/emacs.rs`.

### Prefix key

- `Ctrl-x` enters prefix mode for one keystroke.
- In `Ctrl-x` prefix mode:
  - `Ctrl-s`: save
  - `Ctrl-c`: quit
  - `Ctrl-h`: move cursor to line start
  - `Ctrl-d`: move cursor to line end

### Movement

- `Ctrl-f`: right
- `Ctrl-b`: left
- `Ctrl-n`: down
- `Ctrl-p`: up
- `Ctrl-a`: line start
- `Ctrl-e`: line end
- `Up` / `Down` / `Left` / `Right`
- `Home` / `End`
- `PageUp` / `PageDown`

### Editing

- Character input: insert character
- `Backspace`: delete backward
- `Delete`: delete forward
- `Enter`: newline
- `Tab`: tab
- `Ctrl-d`: delete forward
- `Ctrl-k`: kill line
- `Ctrl-h`: delete backward
- `Ctrl-w`: kill word
- `Ctrl-y`: yank pop
- `Ctrl-t`: transpose characters

### Session / screen / misc

- `Ctrl-o`: trigger save
- `Ctrl-v`: scroll up one
- `Ctrl-l`: clear screen and recenter cursor
- `Ctrl-g`: abort
- `Ctrl-/`, `Ctrl-_`, `Ctrl-?`: undo

## Notes

- This specification is implementation-based. If code and this document diverge, the code is the source of truth until this file is updated.
