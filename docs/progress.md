# Review Mode — Implementation Progress

> Last updated: 2026-06-08

## Goal
Add Review Mode (git diff browsing + code review workflow + proofreading annotations) to the ijevim TUI editor.

## Phases

### ✅ Phase 1: Foundation (Done)
- Data model in `src/review/state.rs` — ReviewState, DiffFile, Hunk, DiffLine, ReviewSubMode, DiffStyle, ReviewArgs parser
- `ReviewKeymap` in `src/keymap/review.rs` — KeymapHandler impl with sub-mode dispatch (FileList, DiffView, Preview, CommentInput, Help)
- Key enum `Keymap::Review` in `src/types.rs`
- `KeymapHandler` factory in `src/keymap/mod.rs` — create_keymap handles Review variant
- `Engine.review_state: Option<ReviewState>` in `src/engine/mod.rs`
- `Editor.enter_review_mode()` / `Editor.exit_review_mode()` with `saved_keymap` restore
- `Editor.current_keymap` + `Editor.saved_keymap` fields (replaces heuristic)
- `:review` command handler in `src/editor/cmd.rs`
- Mock data in `src/review/mock.rs` (separate from `editor/mod.rs` to reduce recompilation)

### ✅ Phase 2: Git Diff Integration (Done)
- `src/review/diff.rs` — git CLI wrapper (`find_git_root`, `run_git_diff`, `parse_unified_diff`)
- `try_get_review_state()` — falls back to mock if not in a git repo
- 10 tests covering: empty diff, simple diff, multi-file, new file, deleted file, git root detection, `hunk_to_patch` reconstruction, stage/unstage integration
- `enter_review_mode()` now calls `try_get_review_state()` → mock fallback

### ✅ Phase 3: Rendering (Done)
- `src/review/layout.rs` — panel layout computation (file list + diff + status bar)
- `src/review/render.rs` — full review UI renderer
  - File list panel with status colors (A/M/D/R)
  - Unified diff with green/red backgrounds for add/delete lines
  - Side-by-side diff with `d` toggle, `[S]` staged indicators
  - Live file preview (reads from disk via `project_root`)
  - Help overlay with all keybindings
  - Status bar (mode, file info, change count, args, diff style)
- `Renderer::render()` updated to delegate to review rendering

### ✅ Phase 4: Hunk Operations (Done)
- `Hunk.staged: bool` — tracks staged state per hunk
- `hunk_to_patch()` — reconstructs unified diff patch from parsed data
- `stage_hunk()` / `unstage_hunk()` — `git apply --cached` / `--cached --reverse` CLI wrappers
- Enter in DiffView toggles staged state + calls git apply
- Visual `[S]` / `[ ]` indicator on hunk headers
- `o`/`p` opens live file preview (reads actual file from disk)
- Integration tests: temp git repo, stage/unstage hunk, verify with `git diff --cached`

### ✅ Phase 5: Annotations (Done)
- `src/review/annotations.rs` — annotation storage module
  - JSON persistence in `.review/annotations.json` (no serde dependency — hand-written JSON)
  - `add_annotation()`, `remove_annotation()`, `toggle_resolved()`, `load_annotations()`
  - Annotation model: `{id, file_path, line, text, resolved, created_at}`
  - Unique ID generation (nanosecond timestamp)
- CommentInput sub-mode:
  - `c` keybinding in DiffView opens a mini-buffer
  - Full character input, backspace, cursor movement (Left/Right/Home/End/Delete)
  - Enter saves (persists annotation) and returns to DiffView
  - Esc cancels (discards buffer)
  - Visual mini-buffer line rendered just above status bar with label + cursor
- Annotation UI in diff view:
  - `[1A]` marker on lines with unresolved annotations (yellow, bold)
  - Count shown for multiple annotations on same line
  - Visible in both unified and side-by-side modes
  - Status bar shows annotation count when > 0

### ✅ Phase 6: Polish & Edge Cases (Done)
- `--staged`/`--cached` argument support — `ReviewArgs::Staged` + `run_git_diff` for `--cached`
- HEAD/file argument support — `ReviewArgs::Against`, `ReviewArgs::File`, `ReviewArgs::Range`
- Edge case handling:
  - Binary files: parse skips them gracefully (no hunks → dropped by `build()`)
  - Empty repos: `find_git_root` returns `None` → auto-falls back to mock data
  - Large diffs: streaming line-by-line parser, display-side truncation
- Status line improvements:
  - Sub-mode indicator (`[files]`, `[diff]`, `[preview]`, `[comment]`, `[help]`)
  - Annotation count shown in status bar when > 0
- Tests: 3 new tests (binary diff skip, mixed binary/text, arg parsing)
  - 18 total review/diff tests, 132 total project-wide

## Key Decisions
- Mock data in `src/review/mock.rs` so `editor/mod.rs` changes don't force full mock recompile
- `[profile.dev] debug = 1` for line-tables-only debug info, saving ~30% compile time
- `.cargo/config.toml` created with mold linker comment for future speedup
- `ReviewKeymap` lives in `src/keymap/review.rs` following existing Vim/Emacs convention
- `enter_review_mode` is sync (git CLI calls fast enough to block briefly)
- `Enter` in DiffView = toggle hunk stage/unstage; `o`/`p` = open preview; `c` = add comment

## Test Status
- 132 tests pass (51 lib unit + 10 main unit + 71 integration)
- 18 review/diff-specific tests (5 annotations + 13 diff)
- `cargo clippy` has toolchain hash mismatch (Nix rustc vs clippy-driver) — not a code issue
- No warnings in `cargo check`
- `cargo check` recommended for fast feedback during development

## Build Configuration
- `Cargo.toml`: `[profile.dev] debug = 1, incremental = true`
- `.cargo/config.toml`: mold linker comment
- Context: `.env` has `CARGO_BUILD_INCREMENTAL=true`
