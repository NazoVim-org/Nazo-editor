# AGENTS.md

## Project
Rust TUI Vim-like editor (ijevim v0.1.0). Single Cargo package, edition 2021.

## Commands
- Build: `cargo build` (debug) / `cargo build --release`
- Run: `cargo run -- [file]`
- Test: `cargo test`
- Lint: `cargo clippy`
- Format: `cargo fmt` (auto) / `cargo fmt --check`

## Critical Gotchas
1. **Outdated files**: README.md, `.github/workflows/ci.yml`, `flake.nix`, `biome.json`, `CLAUDE.md` are leftover from a previous Bun/TypeScript version. This is a Rust project; ignore Bun/TS instructions.
2. **Cargo.lock**: `.gitignore` incorrectly ignores `Cargo.lock`. For executables, commit it (see `.gitignore` comment).
3. **No Node.js/Bun**: No `package.json` exists. Never use `bun`/`npm`/`node`.
4. **CI is broken**: `ci.yml` uses invalid Bun commands. Valid workflow: `release.yml` (triggers on `v*.*.*` tags).
5. **Commits**: Conventional Commits (via `commitlint`).

## Structure
- Entry: `src/main.rs` (tokio + clap CLI)
- Plugins: `src/plugins/` (multi-language: mlua, rust_lisp, quickjs-rusty, libloading)
- Syntax highlighting: `src/highlight/` (syntect)
- Text buffers: `ropey` crate
- TUI: `crossterm` crate
