use serde::Deserialize;
use std::path::PathBuf;

/// Editor configuration loaded from `~/.config/ijevim/config.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default keymap: "vim" or "emacs".
    pub keymap: String,
    /// Show line numbers by default.
    pub show_line_numbers: bool,
    /// Enable soft line wrapping.
    pub wrap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keymap: "vim".to_string(),
            show_line_numbers: true,
            wrap: true,
        }
    }
}

impl Config {
    /// Load config from the default path (`~/.config/ijevim/config.toml`).
    /// Returns `Ok(Config::default())` if the file does not exist or cannot be parsed.
    pub fn load() -> Self {
        let path = config_path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "[config] Ignoring malformed config: {} ({})",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }
}

/// Return the config file path: `~/.config/ijevim/config.toml`
fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("ijevim");
    path.push("config.toml");
    path
}
