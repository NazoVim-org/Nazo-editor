use clap::{Parser, Subcommand};
use ijevim::editor::Editor;
use ijevim::types::Keymap;

#[derive(Parser)]
#[command(name = "ivim")]
#[command(version = "0.1.0")]
#[command(about = "A minimal Vim-like TUI editor written in Rust", long_about = None)]
struct Cli {
    /// Mode command: vim or emacs
    #[command(subcommand)]
    mode: Option<ModeCommand>,

    /// File to edit when no mode command is specified
    file: Option<String>,
}

#[derive(Subcommand)]
enum ModeCommand {
    /// Start in Vim keymap mode
    Vim {
        /// File to edit
        file: Option<String>,
    },
    /// Start in Emacs keymap mode
    Emacs {
        /// File to edit
        file: Option<String>,
    },
}

fn resolve_cli(cli: Cli) -> (Keymap, Option<String>) {
    match cli.mode {
        Some(ModeCommand::Vim { file }) => (Keymap::Vim, file.or(cli.file)),
        Some(ModeCommand::Emacs { file }) => (Keymap::Emacs, file.or(cli.file)),
        None => (Keymap::Vim, cli.file),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt().try_init();

    // Set panic hook to restore terminal state
    std::panic::set_hook(Box::new(|_| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }));

    let cli = Cli::parse();
    let (keymap, file) = resolve_cli(cli);

    let mut editor = Editor::new(file.as_deref(), keymap).await?;
    editor.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vim_subcommand_without_file() {
        let cli = Cli::try_parse_from(["ivim", "vim"]).expect("vim subcommand should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Vim);
        assert_eq!(file, None);
    }

    #[test]
    fn parses_emacs_subcommand_without_file() {
        let cli = Cli::try_parse_from(["ivim", "emacs"]).expect("emacs subcommand should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Emacs);
        assert_eq!(file, None);
    }

    #[test]
    fn parses_vim_subcommand_with_file() {
        let cli = Cli::try_parse_from(["ivim", "vim", "sample.txt"])
            .expect("vim subcommand with file should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Vim);
        assert_eq!(file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn parses_emacs_subcommand_with_file() {
        let cli = Cli::try_parse_from(["ivim", "emacs", "sample.txt"])
            .expect("emacs subcommand with file should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Emacs);
        assert_eq!(file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn parses_top_level_file_as_default_vim() {
        let cli = Cli::try_parse_from(["ivim", "sample.txt"])
            .expect("top-level file argument should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Vim);
        assert_eq!(file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn honors_parent_file_when_mode_subcommand_is_present() {
        let cli = Cli::try_parse_from(["ivim", "sample.txt", "emacs"])
            .expect("parent file before mode subcommand should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Emacs);
        assert_eq!(file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn prefers_subcommand_file_over_parent_file() {
        let cli = Cli::try_parse_from(["ivim", "parent.txt", "vim", "child.txt"])
            .expect("subcommand file should parse");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Vim);
        assert_eq!(file.as_deref(), Some("child.txt"));
    }

    #[test]
    fn treats_unknown_subcommand_as_file_argument() {
        // clap treats unrecognized tokens as positional args (file), not as subcommands.
        // This is expected: `ivim invalid-mode` opens a file named "invalid-mode".
        let cli = Cli::try_parse_from(["ivim", "invalid-mode"])
            .expect("should parse unknown token as file argument");
        let (keymap, file) = resolve_cli(cli);
        assert_eq!(keymap, Keymap::Vim);
        assert_eq!(file.as_deref(), Some("invalid-mode"));
    }
}
