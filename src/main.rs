use ijevim::config::Config;
use ijevim::editor::Editor;
use ijevim::types::{IjevimError, Keymap};

const USAGE: &str =
    "Usage: ivim [<file>] | ivim {vim|emacs} [<file>]\n       ivim --version | --help";

const HELP: &str = "\
ivim — a minimal Vim-like TUI editor

USAGE:
    ivim [<file>]
    ivim vim [<file>]
    ivim emacs [<file>]

OPTIONS:
    -v, --version    Print version and exit
    -h, --help       Print this help message

ARGS:
    <file>          File to edit";

/// Parsed CLI input.
struct Cli {
    keymap: Option<Keymap>,
    file: Option<String>,
}

fn print_help() {
    println!("{}", HELP);
}

fn print_version() {
    println!("ivim {}", env!("CARGO_PKG_VERSION"));
}

/// Parse command-line arguments without external dependencies.
///
/// Accepted shapes:
/// - `ivim`
/// - `ivim <file>`
/// - `ivim vim [<file>]`
/// - `ivim emacs [<file>]`
/// - `ivim <file> vim`        (file first, mode after; preserved by old behaviour)
/// - `ivim <file> vim <file2>` (subcommand file wins)
///
/// Any token that is not a recognised flag or mode is treated as a file path,
/// matching the pre-refactor `clap` behaviour.
fn parse_args() -> Result<Cli, String> {
    let mut keymap: Option<Keymap> = None;
    let mut file: Option<String> = None;
    let mut mode_taken_file: Option<String> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-v" | "--version" => {
                print_version();
                std::process::exit(0);
            }
            "vim" => {
                keymap = Some(Keymap::Vim);
            }
            "emacs" => {
                keymap = Some(Keymap::Emacs);
            }
            _ => {
                // First non-mode token becomes the parent file, unless a mode
                // subcommand already supplied its own file.
                if file.is_none() {
                    file = Some(arg);
                } else if keymap.is_some() && mode_taken_file.is_none() {
                    mode_taken_file = Some(arg);
                } else {
                    // Extra positional: keep first as the canonical file.
                    eprintln!("{}: ignoring extra argument '{}'", USAGE, arg);
                }
            }
        }
    }

    // If a mode subcommand had its own file, prefer it over the parent file.
    if let Some(f) = mode_taken_file {
        file = Some(f);
    }

    Ok(Cli { keymap, file })
}

fn resolve_cli(cli: Cli) -> (Option<Keymap>, Option<String>) {
    (cli.keymap, cli.file)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), IjevimError> {
    // Runtime initialised by #[tokio::main]
    // flavor = "current_thread": we don't need a multi-threaded executor
    // for a single-TUI-editor, and dropping rt-multi-thread keeps tokio smaller.

    // Set panic hook to restore terminal state
    std::panic::set_hook(Box::new(|_| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }));

    let cfg = Config::load();
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}\nerror: {}", USAGE, e);
            std::process::exit(2);
        }
    };
    let (cli_keymap, file) = resolve_cli(cli);
    // CLI keymap takes priority over config keymap
    let keymap = cli_keymap.unwrap_or(match cfg.keymap.as_str() {
        "emacs" => Keymap::Emacs,
        _ => Keymap::Vim,
    });

    let mut editor = Editor::new(file.as_deref(), keymap, cfg).await?;
    editor.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only re-implementation of [`parse_args`] that accepts a custom argv.
    /// This is identical to `parse_args` but without the `std::process::exit`
    /// calls and without consuming the real process arguments.
    fn parse_args_for_test(argv: Vec<String>) -> Result<Cli, String> {
        let mut keymap: Option<Keymap> = None;
        let mut file: Option<String> = None;
        let mut mode_taken_file: Option<String> = None;

        for arg in argv {
            match arg.as_str() {
                "-h" | "--help" | "-v" | "--version" => {
                    return Ok(Cli { keymap, file });
                }
                "vim" => keymap = Some(Keymap::Vim),
                "emacs" => keymap = Some(Keymap::Emacs),
                _ => {
                    if file.is_none() {
                        file = Some(arg);
                    } else if keymap.is_some() && mode_taken_file.is_none() {
                        mode_taken_file = Some(arg);
                    }
                }
            }
        }

        if let Some(f) = mode_taken_file {
            file = Some(f);
        }
        Ok(Cli { keymap, file })
    }

    #[test]
    fn parses_vim_subcommand_without_file() {
        let cli = parse_args_for_test(vec!["vim".into()]).expect("vim should parse");
        assert_eq!(cli.keymap, Some(Keymap::Vim));
        assert_eq!(cli.file, None);
    }

    #[test]
    fn parses_emacs_subcommand_without_file() {
        let cli = parse_args_for_test(vec!["emacs".into()]).expect("emacs should parse");
        assert_eq!(cli.keymap, Some(Keymap::Emacs));
        assert_eq!(cli.file, None);
    }

    #[test]
    fn parses_vim_subcommand_with_file() {
        let cli = parse_args_for_test(vec!["vim".into(), "sample.txt".into()])
            .expect("vim file should parse");
        assert_eq!(cli.keymap, Some(Keymap::Vim));
        assert_eq!(cli.file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn parses_emacs_subcommand_with_file() {
        let cli = parse_args_for_test(vec!["emacs".into(), "sample.txt".into()])
            .expect("emacs file should parse");
        assert_eq!(cli.keymap, Some(Keymap::Emacs));
        assert_eq!(cli.file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn parses_top_level_file_as_default_vim() {
        let cli =
            parse_args_for_test(vec!["sample.txt".into()]).expect("top-level file should parse");
        assert_eq!(cli.keymap, None, "no keymap override when no subcommand");
        assert_eq!(cli.file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn honors_parent_file_when_mode_subcommand_is_present() {
        let cli = parse_args_for_test(vec!["sample.txt".into(), "emacs".into()])
            .expect("parent file + mode should parse");
        assert_eq!(cli.keymap, Some(Keymap::Emacs));
        assert_eq!(cli.file.as_deref(), Some("sample.txt"));
    }

    #[test]
    fn prefers_subcommand_file_over_parent_file() {
        let cli = parse_args_for_test(vec!["parent.txt".into(), "vim".into(), "child.txt".into()])
            .expect("subcommand file should parse");
        assert_eq!(cli.keymap, Some(Keymap::Vim));
        assert_eq!(cli.file.as_deref(), Some("child.txt"));
    }

    #[test]
    fn treats_unknown_subcommand_as_file_argument() {
        let cli = parse_args_for_test(vec!["invalid-mode".into()]).expect("unknown token as file");
        assert_eq!(
            cli.keymap, None,
            "no keymap override for unknown subcommand"
        );
        assert_eq!(cli.file.as_deref(), Some("invalid-mode"));
    }
}
