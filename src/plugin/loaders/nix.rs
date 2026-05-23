use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

pub struct NixPlugin {
    name: String,
    config: NixConfig,
}

#[derive(Debug, Clone)]
pub struct NixConfig {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub repo_url: Option<String>,
}

impl NixConfig {
    pub fn from_nix(code: &str) -> Option<Self> {
        let mut name = None;
        let mut version = None;
        let mut description = None;

        for line in code.lines() {
            let line = line.trim();
            if line.starts_with("name") && line.contains("=") {
                name = line
                    .split('=')
                    .nth(1)
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string());
            } else if line.starts_with("version") && line.contains("=") {
                version = line
                    .split('=')
                    .nth(1)
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string());
            } else if line.starts_with("description") && line.contains("=") {
                description = line
                    .split('=')
                    .nth(1)
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }

        name.map(|name| NixConfig {
            name,
            version,
            description,
            repo_url: None,
        })
    }

    pub fn _from_github(user_repo: &str) -> Result<Self, String> {
        let parts: Vec<&str> = user_repo.split('/').collect();
        if parts.len() != 2 {
            return Err("Invalid format. Expected user/repo".to_string());
        }

        Ok(NixConfig {
            name: user_repo.to_string(),
            version: None,
            description: None,
            repo_url: Some(format!("https://github.com/{}", user_repo)),
        })
    }
}

impl Plugin for NixPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, api: &PluginApi) {
        if let Some(ref repo_url) = self.config.repo_url {
            let _ = self.install_from_github(repo_url, api);
        }
    }

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        false
    }
}

impl NixPlugin {
    fn install_from_github(&self, repo_url: &str, api: &PluginApi) -> Result<(), String> {
        if !repo_url.starts_with("https://github.com/") {
            return Err(format!(
                "Invalid repository URL: only https://github.com/ URLs are allowed, got: {}",
                repo_url
            ));
        }

        api.log(&format!("Installing Nix plugin from {}", repo_url));

        let temp_dir = std::env::temp_dir().join("ijevim-nix");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;

        let output = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                repo_url,
                temp_dir.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Git clone failed: {}", stderr));
        }

        api.log(&format!("Successfully cloned {}", repo_url));
        Ok(())
    }
}

pub struct NixLoader;

impl super::Loader for NixLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["nix"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let code = std::fs::read_to_string(path).map_err(|e| {
            super::LoaderError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let config = NixConfig::from_nix(&code).unwrap_or(NixConfig {
            name: name.clone(),
            version: None,
            description: None,
            repo_url: None,
        });

        let mut plugin = Box::new(NixPlugin {
            name: name.clone(),
            config,
        });

        plugin.setup(&api);

        Ok(plugin)
    }
}
